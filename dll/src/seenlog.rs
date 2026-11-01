//! Seen log: while on, every `interval` ms one line goes to Game\debug-logs\fxr_hunter_seen_<date>_<time>.tsv with
//! the FXRs that read their hunter value during that interval:
//!
//!   ms (since the log started) <TAB> clock (local time) <TAB> speed (game time scale) <TAB> count <TAB> FXR IDs
//!
//! Intervals where nothing was seen are skipped, and so are FXRs the filters exclude (settings.rs).

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde_json::{Value, json};

use crate::{freeze, log, table, win};

struct SeenLog {
    file: BufWriter<File>,
    path: PathBuf,
    interval_ms: u32,
    lines: u64,
}

static LOG: Mutex<Option<SeenLog>> = Mutex::new(None);
/// Incremented on every start/stop, so an old logging thread notices it's been replaced.
static GENERATION: AtomicU64 = AtomicU64::new(0);

fn log_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.join("debug-logs")))
        .unwrap_or_else(|| PathBuf::from("debug-logs"))
}

pub fn start(interval_ms: u32) -> Result<(), String> {
    stop();
    let interval_ms = interval_ms.clamp(10, 60_000);
    let t = win::local_time();
    let name = format!(
        "fxr_hunter_seen_{:04}-{:02}-{:02}_{:02}-{:02}-{:02}.tsv",
        t.year, t.month, t.day, t.hour, t.minute, t.second
    );
    let path = log_dir().join(name);
    let mut file = BufWriter::new(File::create(&path).map_err(|e| format!("{}: {e}", path.display()))?);
    let _ = writeln!(file, "ms\tclock\tspeed\tcount\tfxr_ids");
    let _ = file.flush();
    let start = crate::now_ms();
    *LOG.lock().unwrap() = Some(SeenLog { file, path: path.clone(), interval_ms, lines: 0 });
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    log(&format!("seen log started ({interval_ms} ms): {}", path.display()));
    std::thread::spawn(move || run(generation, interval_ms, start));
    Ok(())
}

pub fn stop() {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    if let Some(mut l) = LOG.lock().unwrap().take() {
        let _ = l.file.flush();
        log(&format!("seen log stopped: {} lines in {}", l.lines, l.path.display()));
    }
}

fn run(generation: u64, interval_ms: u32, start: u32) {
    let t = table();
    let mut last = start;
    loop {
        std::thread::sleep(Duration::from_millis(interval_ms as u64));
        if GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }
        let now = crate::now_ms();
        // Read in (last, now]: seen holds the time of each FXR's latest read.
        let span = now.wrapping_sub(last);
        let seen: Vec<i32> = (0..t.ids.len())
            .filter(|&i| {
                let s = t.seen[i].load(Ordering::Relaxed);
                let d = s.wrapping_sub(last);
                s != 0 && d > 0 && d <= span && !t.excluded[i].load(Ordering::Relaxed)
            })
            .map(|i| t.ids[i])
            .collect();
        last = now;
        if seen.is_empty() {
            continue;
        }
        let c = win::local_time();
        let ids = seen.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(" ");
        let mut guard = LOG.lock().unwrap();
        let Some(l) = guard.as_mut() else { return };
        let _ = writeln!(
            l.file,
            "{}\t{:02}:{:02}:{:02}.{:03}\t{}\t{}\t{ids}",
            now.wrapping_sub(start),
            c.hour,
            c.minute,
            c.second,
            c.milliseconds,
            freeze::scale(),
            seen.len()
        );
        let _ = l.file.flush();
        l.lines += 1;
    }
}

pub fn state() -> Value {
    match LOG.lock().unwrap().as_ref() {
        Some(l) => json!({"active": true, "path": l.path.display().to_string(), "interval_ms": l.interval_ms, "lines": l.lines}),
        None => json!({"active": false, "dir": log_dir().display().to_string()}),
    }
}
