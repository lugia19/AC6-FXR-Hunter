//! Filters for recording and the seen log, and persistent settings.
//!
//! - Ignore list: fxr_hunter_ignore.txt next to the DLL. One FXR ID per line, or a range `a-b`; `#` starts a
//!   comment. Created (with a header) if missing.
//! - ID range: only FXRs from `min` to `max` (inclusive).
//! Excluded FXRs aren't listed by recordings and aren't written to the seen log; FXRs added by ID and looks still
//! work. Settings (both filters and the slow speed) are saved to fxr_hunter_settings.json next to the DLL.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::Ordering;

use serde_json::{Value, json};

use crate::{log, table};

const IGNORE_FILE: &str = "fxr_hunter_ignore.txt";
const SETTINGS_FILE: &str = "fxr_hunter_settings.json";
const IGNORE_HEADER: &str = "# FXRs fxr-hunter doesn't record or log (when the ignore list is on in Settings).\n\
# One FXR ID per line, or a range like 410000-419999. Everything after # is a comment.\n";

struct Settings {
    ignore_enabled: bool,
    range_enabled: bool,
    range_min: i32,
    range_max: i32,
    /// Parsed ignore list: (first, last) inclusive.
    ignore: Vec<(i32, i32)>,
}

static SETTINGS: Mutex<Settings> =
    Mutex::new(Settings { ignore_enabled: false, range_enabled: false, range_min: 0, range_max: i32::MAX, ignore: Vec::new() });

fn path(name: &str) -> Option<PathBuf> {
    crate::dll_dir().map(|d| d.join(name))
}

pub fn ignore_path() -> Option<PathBuf> {
    path(IGNORE_FILE)
}

fn parse_ignore(text: &str) -> Vec<(i32, i32)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let range: Option<(i32, i32)> = match line.split_once('-') {
            Some((a, b)) => a.trim().parse().ok().zip(b.trim().parse().ok()),
            None => line.parse().ok().map(|v| (v, v)),
        };
        match range {
            Some((a, b)) => out.push((a.min(b), a.max(b))),
            None => log(&format!("ignore list: can't read line {line:?}")),
        }
    }
    out
}

/// Recomputes which FXRs are excluded.
fn recompute(s: &Settings) {
    let t = table();
    for (i, &id) in t.ids.iter().enumerate() {
        let ignored = s.ignore_enabled && s.ignore.iter().any(|&(a, b)| (a..=b).contains(&id));
        let outside = s.range_enabled && !(s.range_min..=s.range_max).contains(&id);
        t.excluded[i].store(ignored || outside, Ordering::Relaxed);
    }
}

fn save(s: &Settings) {
    let Some(p) = path(SETTINGS_FILE) else { return };
    let value = json!({
        "ignore_enabled": s.ignore_enabled,
        "range_enabled": s.range_enabled,
        "range_min": s.range_min,
        "range_max": s.range_max,
        "slow_speed": crate::session::freeze_scale(),
    });
    if let Err(e) = std::fs::write(&p, serde_json::to_string_pretty(&value).unwrap_or_default()) {
        log(&format!("settings: can't save {}: {e}", p.display()));
    }
}

fn read_ignore_file() -> Vec<(i32, i32)> {
    let Some(p) = ignore_path() else { return Vec::new() };
    if !p.exists() {
        let _ = std::fs::write(&p, IGNORE_HEADER);
    }
    std::fs::read_to_string(&p).map(|t| parse_ignore(&t)).unwrap_or_default()
}

/// Loads the settings and the ignore list (at startup).
pub fn load() {
    let mut s = SETTINGS.lock().unwrap();
    if let Some(v) = path(SETTINGS_FILE).and_then(|p| std::fs::read_to_string(p).ok()).and_then(|t| serde_json::from_str::<Value>(&t).ok()) {
        s.ignore_enabled = v["ignore_enabled"].as_bool().unwrap_or(false);
        s.range_enabled = v["range_enabled"].as_bool().unwrap_or(false);
        s.range_min = v["range_min"].as_i64().unwrap_or(0) as i32;
        s.range_max = v["range_max"].as_i64().unwrap_or(i32::MAX as i64) as i32;
        if let Some(speed) = v["slow_speed"].as_f64() {
            crate::session::set_freeze_scale_quiet(speed as f32);
        }
    }
    s.ignore = read_ignore_file();
    recompute(&s);
    log(&format!(
        "settings: ignore list {} ({} entries), range {} ({}..={})",
        if s.ignore_enabled { "on" } else { "off" },
        s.ignore.len(),
        if s.range_enabled { "on" } else { "off" },
        s.range_min,
        s.range_max
    ));
}

pub fn set_ignore_enabled(on: bool) {
    let mut s = SETTINGS.lock().unwrap();
    s.ignore_enabled = on;
    recompute(&s);
    save(&s);
}

pub fn set_range(on: bool, min: i32, max: i32) {
    let mut s = SETTINGS.lock().unwrap();
    s.range_enabled = on;
    s.range_min = min.min(max);
    s.range_max = min.max(max);
    recompute(&s);
    save(&s);
}

/// Re-reads the ignore file (after editing it by hand).
pub fn reload_ignore() {
    let mut s = SETTINGS.lock().unwrap();
    s.ignore = read_ignore_file();
    recompute(&s);
    log(&format!("ignore list reloaded: {} entries", s.ignore.len()));
}

/// Appends an FXR ID to the ignore file.
pub fn ignore(fxr_id: i32) -> Result<(), String> {
    let p = ignore_path().ok_or("can't find the DLL's folder")?;
    read_ignore_file(); // creates the file with its header if needed
    let mut f = std::fs::OpenOptions::new().append(true).open(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    let text = std::fs::read_to_string(&p).unwrap_or_default();
    let newline = if text.is_empty() || text.ends_with('\n') { "" } else { "\n" };
    writeln!(f, "{newline}{fxr_id}").map_err(|e| e.to_string())?;
    drop(f);
    reload_ignore();
    Ok(())
}

/// Saves the settings (the slow speed changed).
pub fn save_now() {
    save(&SETTINGS.lock().unwrap());
}

pub fn state() -> Value {
    let s = SETTINGS.lock().unwrap();
    json!({
        "ignore_enabled": s.ignore_enabled,
        "ignore_entries": s.ignore.len(),
        "ignore_path": ignore_path().map(|p| p.display().to_string()),
        "range_enabled": s.range_enabled,
        "range_min": s.range_min,
        "range_max": s.range_max,
    })
}
