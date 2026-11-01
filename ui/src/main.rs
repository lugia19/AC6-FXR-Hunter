//! FXR Hunter UI: record which FXRs show up, give each a look, slow/freeze the game when one appears.
//!
//! The fxr_hunter DLL starts this next to the game (and the game's exit closes it). It talks to the DLL over
//! 127.0.0.1:48690 (dll/src/server.rs): one command per line, one JSON state line back. In-game keys: F9 record,
//! F10 stop, F11 slow on/off; everything else is here, with the mouse.

#![windows_subsystem = "windows"]

use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, RichText, Sense};
use serde_json::Value;

const PORT: u16 = 48690;
const POLL: Duration = Duration::from_millis(250);
/// Name, served look (fxr/inject.mjs FACTORS; 1 = hidden is its own checkbox), swatch.
const COLORS: [(&str, u64, Color32); 7] = [
    ("stock", 0, Color32::TRANSPARENT),
    ("red", 2, Color32::from_rgb(0xff, 0x50, 0x50)),
    ("green", 3, Color32::from_rgb(0x40, 0xd0, 0x40)),
    ("blue", 4, Color32::from_rgb(0x40, 0x80, 0xff)),
    ("yellow", 5, Color32::from_rgb(0xe0, 0xd0, 0x00)),
    ("cyan", 6, Color32::from_rgb(0x20, 0xd0, 0xd0)),
    ("magenta", 7, Color32::from_rgb(0xe0, 0x40, 0xe0)),
];
const MIN_SPEED: f64 = 0.0001;
/// The "waiting for the game" screen only shows after this long without a reply.
const OFFLINE_AFTER: Duration = Duration::from_secs(2);

struct Connection {
    stream: Option<(TcpStream, BufReader<TcpStream>)>,
}

impl Connection {
    /// Sends one command and returns the state, or None if the game isn't reachable.
    fn send(&mut self, command: &str) -> Option<Value> {
        for _ in 0..2 {
            if self.stream.is_none() {
                let addr = SocketAddr::from(([127, 0, 0, 1], PORT));
                let stream = TcpStream::connect_timeout(&addr, Duration::from_millis(300)).ok()?;
                stream.set_read_timeout(Some(Duration::from_secs(1))).ok()?;
                let reader = BufReader::new(stream.try_clone().ok()?);
                self.stream = Some((stream, reader));
            }
            let (stream, reader) = self.stream.as_mut()?;
            let mut line = String::new();
            if writeln!(stream, "{command}").is_ok() && reader.read_line(&mut line).is_ok_and(|n| n > 0) {
                if let Ok(state) = serde_json::from_str(&line) {
                    return Some(state);
                }
            }
            self.stream = None;
        }
        None
    }
}

/// Latest state from the DLL and when it arrived; written by the connection thread.
#[derive(Default)]
struct Shared {
    state: Option<Value>,
    last_ok: Option<Instant>,
}

/// Talks to the DLL off the UI thread, so a slow reply never stalls drawing: sends queued commands, polls the state
/// every POLL otherwise, and keeps the last good state when a reply fails.
///
/// Redraws only when something visible changed (or after a command): every redraw of a window on top of the game
/// makes Windows composite the game's frames, which shows as stutter in the game.
fn connection_thread(commands: Receiver<String>, shared: Arc<Mutex<Shared>>, ctx: egui::Context) {
    let mut conn = Connection { stream: None };
    let mut online = false;
    loop {
        let (command, from_user) = match commands.recv_timeout(POLL) {
            Ok(c) => (c, true),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => ("state".to_string(), false),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
        };
        let mut changed = from_user;
        let mut s = shared.lock().unwrap();
        if let Some(mut state) = conn.send(&command) {
            // Changes on nearly every poll during play; not shown (the rows' dots say it per FXR).
            if let Some(o) = state.as_object_mut() {
                o.remove("reading_count");
            }
            changed |= s.state.as_ref() != Some(&state);
            s.state = Some(state);
            s.last_ok = Some(Instant::now());
        }
        let now_online = s.last_ok.is_some_and(|t| t.elapsed() < OFFLINE_AFTER);
        changed |= now_online != online;
        online = now_online;
        drop(s);
        if changed {
            ctx.request_repaint();
        }
    }
}

struct App {
    commands: Sender<String>,
    shared: Arc<Mutex<Shared>>,
    add_id: String,
    speed: f64,
    speed_synced: bool,
    log_open: bool,
    log_interval: u32,
    settings_open: bool,
    range_min: i64,
    range_max: i64,
    range_synced: bool,
    /// Rows ticked for bulk actions, and the last clicked one (Shift+click range start).
    selection: BTreeSet<i64>,
    anchor: Option<i64>,
}

fn ids(v: &Value) -> Vec<i64> {
    v.as_array().map(|a| a.iter().filter_map(Value::as_i64).collect()).unwrap_or_default()
}

impl App {
    fn status_line(state: &Value) -> String {
        let mut parts = vec![if state["recording"].as_bool() == Some(true) { "RECORDING" } else { "not recording" }.to_string()];
        let scale = state["scale"].as_f64().unwrap_or(1.0);
        if state["freeze_available"].as_bool() != Some(true) {
            parts.push("slow unavailable".into());
        } else if scale < 1.0 {
            let label = if scale <= MIN_SPEED { "FROZEN".to_string() } else { format!("SLOWED ×{scale}") };
            parts.push(match state["triggered"].as_i64() {
                Some(id) => format!("{label} (FXR {id} appeared)"),
                None => label,
            });
        }
        parts.push(format!("{} injected", state["total_ids"]));
        parts.join(" | ")
    }

    /// Selects/deselects a row; with Shift, everything between it and the last clicked row too.
    fn click_select(&mut self, ui: &egui::Ui, order: &[i64], id: i64, select: bool) {
        let shift = ui.input(|i| i.modifiers.shift);
        let range = self
            .anchor
            .filter(|_| shift)
            .and_then(|a| Some((order.iter().position(|&x| x == a)?, order.iter().position(|&x| x == id)?)));
        match range {
            Some((x, y)) => {
                for &r in &order[x.min(y)..=x.max(y)] {
                    if select {
                        self.selection.insert(r);
                    } else {
                        self.selection.remove(&r);
                    }
                }
            }
            None if select => {
                self.selection.insert(id);
            }
            None => {
                self.selection.remove(&id);
            }
        }
        self.anchor = Some(id);
    }

    /// Bulk actions on the selected rows.
    fn bulk_bar(&mut self, ui: &mut egui::Ui, cmds: &mut Vec<String>) {
        let sel: Vec<i64> = self.selection.iter().copied().collect();
        ui.horizontal(|ui| {
            if sel.is_empty() {
                ui.label(RichText::new("Tick rows (Shift: a range, Ctrl+A: all) to change several at once.").weak());
                return;
            }
            ui.label(RichText::new(format!("{} selected:", sel.len())).strong());
            egui::ComboBox::from_id_salt("bulk color").width(80.0).selected_text("color…").show_ui(ui, |ui| {
                for (name, code, _) in COLORS {
                    if ui.selectable_label(false, name).clicked() {
                        cmds.extend(sel.iter().map(|id| format!("look {id} {code}")));
                    }
                }
            });
            if ui.button("Hide").clicked() {
                cmds.extend(sel.iter().map(|id| format!("hide {id} 1")));
            }
            if ui.button("Show").clicked() {
                cmds.extend(sel.iter().map(|id| format!("hide {id} 0")));
            }
            if ui.button("Solo").on_hover_text("Hide every other listed FXR").clicked() {
                cmds.push(format!("solo {}", sel.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(" ")));
            }
            if ui.button("Copy IDs").on_hover_text("One per line").clicked() {
                ui.ctx().copy_text(sel.iter().map(|id| id.to_string()).collect::<Vec<_>>().join("\n"));
            }
            if ui.button("Ignore").on_hover_text("Add them to the ignore list (Settings)").clicked() {
                cmds.extend(sel.iter().map(|id| format!("ignore {id}")));
                self.selection.clear();
            }
            if ui.button("×").on_hover_text("Remove them from the list").clicked() {
                cmds.extend(sel.iter().map(|id| format!("remove {id}")));
                self.selection.clear();
            }
            if ui.button("Clear selection").clicked() {
                self.selection.clear();
            }
        });
    }

    fn list(&mut self, ui: &mut egui::Ui, state: &Value, cmds: &mut Vec<String>) {
        let rows = state["list"].as_array().cloned().unwrap_or_default();
        let order: Vec<i64> = rows.iter().filter_map(|r| r["id"].as_i64()).collect();
        // Rows that left the list (new recording, filters, x) leave the selection.
        self.selection.retain(|id| order.contains(id));
        if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::A)) && !ui.ctx().wants_keyboard_input() {
            self.selection.extend(order.iter().copied());
        }
        egui::Grid::new("fxrs").striped(true).spacing([8.0, 4.0]).show(ui, |ui| {
            let mut all = !order.is_empty() && order.iter().all(|id| self.selection.contains(id));
            if ui.checkbox(&mut all, "").on_hover_text("Select all / none").changed() {
                if all {
                    self.selection.extend(order.iter().copied());
                } else {
                    self.selection.clear();
                }
            }
            for h in ["FXR", "first", "", "color", "", "", "", "", "", "", ""] {
                ui.label(RichText::new(h).weak());
            }
            ui.end_row();
            for row in &rows {
                let id = row["id"].as_i64().unwrap_or(0);
                let hidden = row["hidden"].as_bool() == Some(true);
                let look = row["look"].as_u64().unwrap_or(0);

                let mut selected = self.selection.contains(&id);
                if ui.checkbox(&mut selected, "").changed() {
                    self.click_select(ui, &order, id, selected);
                }
                let text = RichText::new(id.to_string()).monospace().size(14.0);
                let text = if hidden { text.weak() } else { text.strong() };
                let text = if self.selection.contains(&id) { text.background_color(ui.visuals().selection.bg_fill) } else { text };
                if ui.add(egui::Label::new(text).sense(Sense::click())).on_hover_text("Click to select (Shift: range)").clicked() {
                    let select = !self.selection.contains(&id);
                    self.click_select(ui, &order, id, select);
                }
                ui.label(match row["first_ms"].as_u64() {
                    Some(ms) => format!("+{:.1}s", ms as f64 / 1000.0),
                    None => "added".into(),
                });
                let reading = row["reading"].as_bool() == Some(true);
                ui.label(RichText::new("●").color(if reading { Color32::from_rgb(0x20, 0xc0, 0x20) } else { Color32::GRAY }))
                    .on_hover_text(if reading { "Reading its value now" } else { "Not reading now" });

                let current = COLORS.iter().find(|c| c.1 == look).unwrap_or(&COLORS[0]);
                egui::ComboBox::from_id_salt(("color", id)).width(80.0).selected_text(current.0).show_ui(ui, |ui| {
                    for (name, code, _) in COLORS {
                        if ui.selectable_label(code == look, name).clicked() && code != look {
                            cmds.push(format!("look {id} {code}"));
                        }
                    }
                });
                let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), Sense::hover());
                if current.2 != Color32::TRANSPARENT {
                    ui.painter().rect_filled(rect, 2.0, current.2);
                }

                let mut h = hidden;
                if ui.checkbox(&mut h, "hidden").changed() {
                    cmds.push(format!("hide {id} {}", h as u8));
                }
                if ui.button("Solo").on_hover_text("Hide every other listed FXR").clicked() {
                    cmds.push(format!("solo {id}"));
                }
                let mut watch = row["watch"].as_bool() == Some(true);
                if ui.checkbox(&mut watch, "slow on appear").changed() {
                    cmds.push(format!("watch {id}"));
                }
                if ui.button("Copy").clicked() {
                    ui.ctx().copy_text(id.to_string());
                }
                if ui.button("Ignore").on_hover_text("Add to the ignore list (Settings)").clicked() {
                    cmds.push(format!("ignore {id}"));
                }
                let hint = if row["added"].as_bool() == Some(true) {
                    "Remove from the list"
                } else {
                    "Remove from the list until the next recording"
                };
                if ui.button("×").on_hover_text(hint).clicked() {
                    cmds.push(format!("remove {id}"));
                }
                ui.end_row();
            }
        });
        if rows.is_empty() {
            ui.label(RichText::new("Nothing recorded yet: F9 (or Record) and do something in game.").weak());
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut cmds: Vec<String> = Vec::new();

        let (state, online) = {
            let s = self.shared.lock().unwrap();
            (s.state.clone(), s.last_ok.is_some_and(|t| t.elapsed() < OFFLINE_AFTER))
        };
        let Some(state) = state.filter(|_| online) else {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.heading(format!("Waiting for the game (fxr_hunter DLL on port {PORT})..."));
            });
            return;
        };
        if !self.speed_synced {
            // The DLL keeps the slow speed across UI restarts.
            let s = state["freeze_scale"].as_f64().unwrap_or(MIN_SPEED);
            self.speed = if s <= MIN_SPEED { 0.0 } else { s };
            self.speed_synced = true;
        }

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.label(RichText::new(Self::status_line(&state)).strong());
            let message = match state["error"].as_str() {
                Some(e) => format!("Error: {e}"),
                None => state["message"].as_str().unwrap_or("").to_string(),
            };
            // Always takes its line, so the list below doesn't jump when a message comes or goes.
            ui.label(if message.is_empty() { " ".to_string() } else { message });
            ui.horizontal(|ui| {
                for (text, cmd) in [
                    ("Record (F9)", "record"),
                    ("Stop (F10)", "stop"),
                    ("Toggle slow (F11)", "freeze"),
                    ("Unhide all", "unhide"),
                    ("Reset looks", "reset"),
                ] {
                    if ui.button(text).clicked() {
                        cmds.push(cmd.into());
                    }
                }
                let logging = state["log"]["active"].as_bool() == Some(true);
                if ui.button(if logging { "Log… (on)" } else { "Log…" }).clicked() {
                    self.log_open = !self.log_open;
                }
                if ui.button("Settings…").clicked() {
                    self.settings_open = !self.settings_open;
                }
            });
            ui.horizontal(|ui| {
                ui.label("Slow speed (F11 and slow on appear):");
                let r = ui.add(egui::DragValue::new(&mut self.speed).range(0.0..=0.95).speed(0.005).max_decimals(4));
                if r.changed() {
                    cmds.push(format!("freezescale {}", self.speed.max(MIN_SPEED)));
                }
                ui.label(RichText::new("(0 = frozen)").weak());
            });
            ui.add_space(4.0);
        });

        egui::TopBottomPanel::bottom("bottom").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Add FXR ID:");
                let r = ui.add(egui::TextEdit::singleline(&mut self.add_id).desired_width(100.0));
                let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if (ui.button("Add").clicked() || enter) && !self.add_id.trim().is_empty() {
                    cmds.push(format!("add {}", self.add_id.trim()));
                    self.add_id.clear();
                }
            });
            // Looks/watches on FXRs that aren't in the list (left over from an earlier recording).
            let listed: Vec<i64> = state["list"].as_array().map(|a| a.iter().filter_map(|r| r["id"].as_i64()).collect()).unwrap_or_default();
            let mut others: Vec<String> = state["looks"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|p| Some((p[0].as_i64()?, p[1].as_u64()?)))
                .filter(|(id, _)| !listed.contains(id))
                .map(|(id, m)| format!("{id}: {}", COLORS.iter().find(|c| c.1 == m).map_or("?", |c| c.0)))
                .collect();
            others.extend(ids(&state["hidden"]).into_iter().filter(|id| !listed.contains(id)).map(|id| format!("{id}: hidden")));
            if !others.is_empty() {
                ui.label(RichText::new(format!("Looks on unlisted FXRs: {}", others.join(", "))).weak());
            }
            let watches: Vec<String> = ids(&state["watches"]).into_iter().filter(|id| !listed.contains(id)).map(|id| id.to_string()).collect();
            if !watches.is_empty() {
                ui.label(RichText::new(format!("Slow on appear (unlisted): {}", watches.join(", "))).weak());
            }
            ui.add_space(4.0);
        });

        if self.log_open {
            let log = &state["log"];
            let active = log["active"].as_bool() == Some(true);
            let mut open = true;
            egui::Window::new("Seen log").open(&mut open).resizable(false).collapsible(false).show(ctx, |ui| {
                ui.label("Writes the FXRs seen in each interval to a TSV file, with timestamps.");
                ui.horizontal(|ui| {
                    ui.label("Every");
                    ui.add_enabled(!active, egui::DragValue::new(&mut self.log_interval).range(10..=60_000).speed(10).suffix(" ms"));
                    if active {
                        if ui.button("Stop").clicked() {
                            cmds.push("log stop".into());
                        }
                    } else if ui.button("Start").clicked() {
                        cmds.push(format!("log start {}", self.log_interval));
                    }
                });
                let path = log["path"].as_str().or(log["dir"].as_str()).unwrap_or("").to_string();
                if active {
                    ui.label(format!("Logging every {} ms: {} lines", log["interval_ms"], log["lines"]));
                }
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&path).monospace().small());
                    if !path.is_empty() && ui.button("Open folder").clicked() {
                        let arg = if active { format!("/select,{path}") } else { path.clone() };
                        let _ = std::process::Command::new("explorer").arg(arg).spawn();
                    }
                });
                ui.label(RichText::new("Columns: ms, clock, speed, count, FXR IDs. Empty intervals are skipped.").weak().small());
            });
            self.log_open = open;
        }

        if self.settings_open {
            let st = &state["settings"];
            if !self.range_synced {
                self.range_min = st["range_min"].as_i64().unwrap_or(0);
                self.range_max = st["range_max"].as_i64().unwrap_or(999_999_999).min(2_147_483_647);
                self.range_synced = true;
            }
            let mut open = true;
            egui::Window::new("Settings").open(&mut open).resizable(false).collapsible(false).show(ctx, |ui| {
                ui.label(RichText::new("Filters for recording and the seen log (added FXRs and looks still work).").weak());
                ui.separator();
                let mut ignore_on = st["ignore_enabled"].as_bool() == Some(true);
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut ignore_on, "Ignore list").changed() {
                        cmds.push(format!("settings ignore {}", ignore_on as u8));
                    }
                    ui.label(RichText::new(format!("{} entries", st["ignore_entries"])).weak());
                });
                let path = st["ignore_path"].as_str().unwrap_or("").to_string();
                ui.label(RichText::new(&path).monospace().small());
                ui.horizontal(|ui| {
                    if ui.button("Open file").clicked() && !path.is_empty() {
                        let _ = std::process::Command::new("notepad").arg(&path).spawn();
                    }
                    if ui.button("Reload").on_hover_text("Re-read the file after editing it").clicked() {
                        cmds.push("ignore reload".into());
                    }
                });
                ui.label(RichText::new("One FXR ID per line, or a range like 410000-419999; # starts a comment. The Ignore button on a row adds it.").weak().small());
                ui.separator();
                let mut range_on = st["range_enabled"].as_bool() == Some(true);
                let mut changed = false;
                ui.horizontal(|ui| {
                    changed |= ui.checkbox(&mut range_on, "Only FXR IDs from").changed();
                    changed |= ui.add(egui::DragValue::new(&mut self.range_min).range(0..=2_147_483_647).speed(100)).changed();
                    ui.label("to");
                    changed |= ui.add(egui::DragValue::new(&mut self.range_max).range(0..=2_147_483_647).speed(100)).changed();
                });
                if changed {
                    cmds.push(format!("settings range {} {} {}", range_on as u8, self.range_min, self.range_max));
                }
            });
            self.settings_open = open;
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            self.bulk_bar(ui, &mut cmds);
            ui.separator();
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| self.list(ui, &state, &mut cmds));
        });

        for cmd in cmds {
            let _ = self.commands.send(cmd);
        }
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        // wgpu (D3D12/Vulkan), not glow: the OpenGL backend flickered next to the game.
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_title("FXR Hunter")
            .with_inner_size([720.0, 640.0])
            .with_always_on_top(),
        ..Default::default()
    };
    eframe::run_native(
        "FXR Hunter",
        options,
        Box::new(|cc| {
            let (commands, receiver) = channel();
            let shared = Arc::new(Mutex::new(Shared::default()));
            let ctx = cc.egui_ctx.clone();
            let thread_shared = shared.clone();
            std::thread::spawn(move || connection_thread(receiver, thread_shared, ctx));
            Ok(Box::new(App {
                commands,
                shared,
                add_id: String::new(),
                speed: 0.0,
                speed_synced: false,
                log_open: false,
                log_interval: 100,
                settings_open: false,
                range_min: 0,
                range_max: 999_999_999,
                range_synced: false,
                selection: BTreeSet::new(),
                anchor: None,
            }))
        }),
    )
}
