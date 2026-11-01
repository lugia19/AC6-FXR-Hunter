//! Recording sessions and per-FXR looks.
//!
//! F9 starts recording: every FXR that reads its hunter value gets listed (sorted by ID) until F10 stops it; FXRs can
//! also be added to the list by ID (they stay across recordings). Each FXR can get a color (stock, red, green, blue,
//! yellow, cyan, magenta), be hidden, or be watched: the next time it appears (starts reading after a gap), the game
//! slows down to the slow speed (0.0001 = frozen). Solo hides every other listed FXR. Looks stay until reset, so the
//! player can repeat the action and adjust. Some effects only read when they emit particles, so a new look may only
//! show on particles emitted after the change.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

use serde_json::{Value, json};

use crate::{freeze, log, table};

// Looks (served modes, fxr/inject.mjs FACTORS): 0 stock, 1 hidden, 2 red, 3 green, 4 blue, 5 yellow, 6 cyan, 7 magenta.
const HIDDEN: u8 = 1;
/// An FXR counts as reading now if it read its value this recently.
pub const LIVE_WINDOW_MS: u32 = 500;
/// Game speed while "frozen" (f32 bits): 0.0001 by default (not 0: some game code may divide by the frame time), or
/// any slow motion the player sets (e.g. 0.1).
static FREEZE_SCALE: AtomicU32 = AtomicU32::new(0x38D1_B717); // 0.0001f32
const MIN_SCALE: f32 = 0.0001;

pub fn freeze_scale() -> f32 {
    f32::from_bits(FREEZE_SCALE.load(Ordering::Relaxed))
}

fn frozen() -> bool {
    freeze::scale() < 1.0
}

/// Sets the slow speed (clamped to 0.0001..0.95) without applying or saving it (settings load).
pub fn set_freeze_scale_quiet(scale: f32) {
    let scale = if scale.is_finite() { scale.clamp(MIN_SCALE, 0.95) } else { MIN_SCALE };
    FREEZE_SCALE.store(scale.to_bits(), Ordering::Relaxed);
}

/// Sets the slow speed; applies right away if slowed, and saves the settings.
pub fn set_freeze_scale(scale: f32) {
    set_freeze_scale_quiet(scale);
    if frozen() {
        freeze::set_scale(freeze_scale());
    }
    crate::settings::save_now();
}

struct Session {
    /// now_ms() when the current/last recording started (0 = never recorded).
    start: u32,
    recording: bool,
    /// Colors (2-7); absent = stock.
    looks: HashMap<usize, u8>,
    hidden: HashSet<usize>,
    /// Added to the list by ID (kept across recordings).
    added: HashSet<usize>,
    /// Recorded FXRs removed from the list (until the next recording).
    dismissed: HashSet<usize>,
    message: String,
}

static SESSION: Mutex<Option<Session>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut Session) -> R) -> R {
    let mut guard = SESSION.lock().unwrap();
    let s = guard.get_or_insert_with(|| Session {
        start: 0,
        recording: false,
        looks: HashMap::new(),
        hidden: HashSet::new(),
        added: HashSet::new(),
        dismissed: HashSet::new(),
        message: String::new(),
    });
    f(s)
}

/// Seen during the latest recording, and not filtered out (ignore list / ID range).
fn recorded(s: &Session, i: usize) -> bool {
    let t = table();
    s.start != 0
        && t.first[i].load(Ordering::Relaxed) >= s.start
        && !t.excluded[i].load(Ordering::Relaxed)
        && !s.dismissed.contains(&i)
}

/// The listed FXRs (recorded or added), by ID.
fn list(s: &Session) -> Vec<usize> {
    // Indices follow the sorted ID table, so index order is ID order.
    (0..table().ids.len()).filter(|&i| s.added.contains(&i) || recorded(s, i)).collect()
}

/// Adds an FXR to the list by ID.
pub fn add(fxr_id: i32) -> bool {
    let Some(i) = crate::index_of(fxr_id) else { return false };
    with(|s| {
        s.added.insert(i);
        s.dismissed.remove(&i);
    });
    true
}

/// Removes an FXR from the list: an added one for good, a recorded one until the next recording.
pub fn remove(fxr_id: i32) -> bool {
    let Some(i) = crate::index_of(fxr_id) else { return false };
    with(|s| {
        s.added.remove(&i);
        s.dismissed.insert(i);
    });
    true
}

/// Writes every FXR's mode.
fn apply(s: &Session) {
    let t = table();
    for i in 0..t.ids.len() {
        let mode = if s.hidden.contains(&i) {
            HIDDEN
        } else {
            s.looks.get(&i).copied().unwrap_or(0)
        };
        t.modes[i].store(mode, Ordering::Relaxed);
    }
}

fn id_of(i: usize) -> i32 {
    table().ids[i]
}

// ---- Recording ----

pub fn record() {
    let now = crate::now_ms();
    crate::RECORD_START.store(now, Ordering::Relaxed);
    with(|s| {
        s.start = now;
        s.recording = true;
        s.dismissed.clear();
        s.message = "Recording: every FXR that shows up gets listed. F10 stops.".into();
        apply(s);
    });
    log("recording started");
}

pub fn stop() {
    crate::RECORD_START.store(0, Ordering::Relaxed);
    with(|s| {
        if s.recording {
            s.recording = false;
            let ids: Vec<i32> = list(s).into_iter().map(id_of).collect();
            s.message = format!("Recorded {} FXRs.", ids.len());
            log(&format!("recording stopped: {} FXRs: {ids:?}", ids.len()));
        }
    });
}

// ---- Looks ----

pub fn set_look(fxr_id: i32, mode: u8) -> bool {
    let Some(i) = crate::index_of(fxr_id) else { return false };
    with(|s| {
        match mode {
            0 => {
                s.looks.remove(&i);
            }
            HIDDEN => {
                s.hidden.insert(i);
            }
            _ => {
                s.looks.insert(i, mode);
            }
        }
        apply(s);
    });
    true
}

/// Hides/shows an FXR (`hide`: None = toggle).
pub fn set_hidden(fxr_id: i32, hide: Option<bool>) -> bool {
    let Some(i) = crate::index_of(fxr_id) else { return false };
    with(|s| {
        let hide = hide.unwrap_or(!s.hidden.contains(&i));
        if hide {
            s.hidden.insert(i);
        } else {
            s.hidden.remove(&i);
        }
        apply(s);
    });
    true
}

/// Hides every listed FXR except these (and shows these).
pub fn solo(fxr_ids: &[i32]) -> bool {
    let Some(keep) = fxr_ids.iter().map(|&id| crate::index_of(id)).collect::<Option<Vec<usize>>>() else { return false };
    with(|s| {
        for j in list(s) {
            s.hidden.insert(j);
        }
        for i in &keep {
            s.hidden.remove(i);
        }
        s.message = match fxr_ids {
            [one] => format!("Solo: FXR {one}"),
            _ => format!("Solo: {} FXRs", fxr_ids.len()),
        };
        apply(s);
    });
    true
}

pub fn unhide_all() {
    with(|s| {
        s.hidden.clear();
        apply(s);
    });
}

pub fn reset_looks() {
    with(|s| {
        s.looks.clear();
        s.hidden.clear();
        s.message = "Looks reset.".into();
        apply(s);
    });
}

// ---- Watch / freeze ----

/// Arms/disarms "freeze when this FXR appears".
pub fn toggle_watch(fxr_id: i32) -> bool {
    let Some(i) = crate::index_of(fxr_id) else { return false };
    let w = &table().watch[i];
    let armed = !w.load(Ordering::Relaxed);
    w.store(armed, Ordering::Relaxed);
    log(&format!("watch {fxr_id}: {}", if armed { "armed" } else { "off" }));
    true
}

/// Called from the read hook when a watched FXR appears (it has already been disarmed).
pub fn watch_triggered(i: usize) {
    crate::TRIGGERED.store(id_of(i), Ordering::Relaxed);
    freeze::set_scale(freeze_scale());
    log(&format!("FXR {} appeared: frozen (speed {})", id_of(i), freeze_scale()));
}

pub fn toggle_freeze() {
    freeze::set_scale(if frozen() { 1.0 } else { freeze_scale() });
    if !frozen() {
        crate::TRIGGERED.store(-1, Ordering::Relaxed);
    }
}

pub fn state() -> Value {
    let t = table();
    let now = crate::now_ms();
    let reading = |i: usize| {
        let seen = t.seen[i].load(Ordering::Relaxed);
        seen != 0 && now.wrapping_sub(seen) < LIVE_WINDOW_MS
    };
    with(|s| {
        let rows: Vec<Value> = list(s)
            .into_iter()
            .map(|i| {
                json!({
                    "id": id_of(i),
                    "first_ms": recorded(s, i).then(|| t.first[i].load(Ordering::Relaxed).wrapping_sub(s.start)),
                    "added": s.added.contains(&i),
                    "reading": reading(i),
                    "look": s.looks.get(&i).copied().unwrap_or(0),
                    "hidden": s.hidden.contains(&i),
                    "watch": t.watch[i].load(Ordering::Relaxed),
                })
            })
            .collect();
        let others = |set: Vec<usize>| set.into_iter().map(id_of).collect::<Vec<_>>();
        let triggered = crate::TRIGGERED.load(Ordering::Relaxed);
        json!({
            "recording": s.recording,
            "scale": freeze::scale(),
            "freeze_available": freeze::available(),
            "triggered": if triggered >= 0 { Some(triggered) } else { None },
            "message": s.message,
            "list": rows,
            "looks": s.looks.iter().map(|(&i, &m)| json!([id_of(i), m])).collect::<Vec<_>>(),
            "watches": others((0..t.ids.len()).filter(|&i| t.watch[i].load(Ordering::Relaxed)).collect()),
            "hidden": others(s.hidden.iter().copied().collect()),
            "freeze_scale": freeze_scale(),
            "reading_count": (0..t.ids.len()).filter(|&i| reading(i)).count(),
            "total_ids": t.ids.len(),
        })
    })
}
