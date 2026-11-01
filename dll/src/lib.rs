//! fxr_hunter: find which FXR an effect on screen is.
//!
//! Every FXR in the mod package reads a custom external value, HUNTER_BASE + its own ID (fxr/inject.mjs), that picks
//! how it's drawn (stock, hidden or one of six tints). This DLL hooks the function effects read external values
//! through (0x142b56480, 1.09.1) and:
//! - records when each FXR read its value (= it's alive right now) and when it first did during a recording,
//! - serves each FXR's look,
//! - freezes the game (freeze.rs) when a watched FXR appears.
//! Recording, looks and watches: session.rs. The UI (ui/: fxr_hunter_ui.exe, started by launch_ui and closed with
//! the game) talks to it through a local socket (server.rs).
//!
//! Keys (game window focused): F9 record, F10 stop recording, F11 slow on/off, Up/Down select a listed FXR (marked in
//! the UI), Insert cycle its color, Delete hide/show it, Home solo it, Page Up slow when it appears, Page Down reset
//! all looks.
//! Log: Game\debug-logs\fxr_hunter.log

mod freeze;
mod input;
mod seenlog;
mod server;
mod session;
mod settings;
mod win;

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicPtr, AtomicU8, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use retour::RawDetour;

pub const HUNTER_BASE: i32 = 1_000_000_000; // must match fxr/inject.mjs
const IDS_FILE: &str = "fxr_hunter_ids.txt";
const UI_EXE: &str = "fxr_hunter_ui.exe";

const GET_ADDR: usize = 0x142b56480;
const GET_PROLOGUE: [u8; 5] = [0x44, 0x89, 0x44, 0x24, 0x18]; // mov [rsp+18],r8d

pub struct Table {
    /// Injected FXR IDs, sorted.
    pub ids: Vec<i32>,
    /// Mode served to each FXR (0 stock, 1 hidden, 2-7 tints).
    pub modes: Vec<AtomicU8>,
    /// now_ms() of each FXR's last read (0 = never).
    pub seen: Vec<AtomicU32>,
    /// now_ms() of each FXR's first read during the latest recording (older values: not seen in it).
    pub first: Vec<AtomicU32>,
    /// Freeze the next time this FXR appears (one-shot).
    pub watch: Vec<AtomicBool>,
    /// Filtered out of recordings and the seen log (ignore list / ID range, settings.rs).
    pub excluded: Vec<AtomicBool>,
}

static TABLE: OnceLock<Table> = OnceLock::new();
static EMPTY: OnceLock<Table> = OnceLock::new();
static ORIG_GET: AtomicUsize = AtomicUsize::new(0);
static NOW: AtomicU32 = AtomicU32::new(1);
/// now_ms() when the current recording started, 0 when not recording.
pub static RECORD_START: AtomicU32 = AtomicU32::new(0);
/// The watched FXR that froze the game, -1 if none.
pub static TRIGGERED: AtomicI32 = AtomicI32::new(-1);
/// A watched FXR "appears" when it reads its value after not reading for this long.
const APPEAR_GAP_MS: u32 = 500;
static MODULE: AtomicPtr<u8> = AtomicPtr::new(std::ptr::null_mut());
static LOG: OnceLock<Mutex<File>> = OnceLock::new();

pub fn table() -> &'static Table {
    TABLE.get().unwrap_or_else(|| EMPTY.get_or_init(|| Table { ids: Vec::new(), modes: Vec::new(), seen: Vec::new(), first: Vec::new(), watch: Vec::new(), excluded: Vec::new() }))
}

pub fn index_of(fxr_id: i32) -> Option<usize> {
    table().ids.binary_search(&fxr_id).ok()
}

pub fn now_ms() -> u32 {
    NOW.load(Ordering::Relaxed)
}

pub fn log(msg: &str) {
    if let Some(Ok(mut f)) = LOG.get().map(|l| l.lock()) {
        let _ = writeln!(f, "{msg}");
    }
}

unsafe extern "system" fn get_impl(block: *mut u8, out: *mut u32, id: i32) -> *mut u32 {
    let orig: unsafe extern "system" fn(*mut u8, *mut u32, i32) -> *mut u32 =
        unsafe { std::mem::transmute(ORIG_GET.load(Ordering::Relaxed)) };
    let result = unsafe { orig(block, out, id) };
    if id >= HUNTER_BASE && !out.is_null() {
        if let Some(t) = TABLE.get() {
            if let Ok(i) = t.ids.binary_search(&(id - HUNTER_BASE)) {
                let now = NOW.load(Ordering::Relaxed);
                let previous = t.seen[i].swap(now, Ordering::Relaxed);
                let start = RECORD_START.load(Ordering::Relaxed);
                if start != 0 && t.first[i].load(Ordering::Relaxed) < start {
                    t.first[i].store(now, Ordering::Relaxed);
                }
                if t.watch[i].load(Ordering::Relaxed)
                    && (previous == 0 || now.wrapping_sub(previous) > APPEAR_GAP_MS)
                    && t.watch[i].swap(false, Ordering::Relaxed)
                {
                    session::watch_triggered(i);
                }
                unsafe { *out = (t.modes[i].load(Ordering::Relaxed) as f32).to_bits() };
            }
        }
    }
    result
}

fn install_get_hook() -> Result<(), String> {
    let prologue = unsafe { std::slice::from_raw_parts(GET_ADDR as *const u8, 5) };
    if prologue != GET_PROLOGUE {
        return Err(format!("unexpected bytes at {GET_ADDR:#x} (not 1.09.1?)"));
    }
    let detour = unsafe { RawDetour::new(GET_ADDR as *const (), get_impl as *const ()) }.map_err(|e| e.to_string())?;
    ORIG_GET.store(detour.trampoline() as *const () as usize, Ordering::Relaxed);
    unsafe { detour.enable() }.map_err(|e| e.to_string())?;
    std::mem::forget(detour);
    Ok(())
}

pub(crate) fn dll_dir() -> Option<PathBuf> {
    let mut buf = vec![0u16; 1024];
    let n = unsafe { win::GetModuleFileNameW(MODULE.load(Ordering::Relaxed), buf.as_mut_ptr(), buf.len() as u32) };
    if n == 0 {
        return None;
    }
    PathBuf::from(String::from_utf16_lossy(&buf[..n as usize])).parent().map(|p| p.to_path_buf())
}

fn load_ids() -> Result<usize, String> {
    let path = dll_dir().ok_or("can't find the DLL's folder")?.join(IDS_FILE);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut ids: Vec<i32> = text.lines().filter_map(|l| l.trim().parse().ok()).collect();
    ids.sort_unstable();
    ids.dedup();
    let n = ids.len();
    let modes = (0..n).map(|_| AtomicU8::new(0)).collect();
    let seen = (0..n).map(|_| AtomicU32::new(0)).collect();
    let first = (0..n).map(|_| AtomicU32::new(0)).collect();
    let watch = (0..n).map(|_| AtomicBool::new(false)).collect();
    let excluded = (0..n).map(|_| AtomicBool::new(false)).collect();
    let _ = TABLE.set(Table { ids, modes, seen, first, watch, excluded });
    Ok(n)
}

fn ticker() {
    let start = Instant::now();
    loop {
        std::thread::sleep(Duration::from_millis(5));
        NOW.store(start.elapsed().as_millis() as u32 + 1, Ordering::Relaxed);
    }
}

fn init() {
    let dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.join("debug-logs")))
        .unwrap_or_else(|| PathBuf::from("debug-logs"));
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(f) = OpenOptions::new().create(true).write(true).truncate(true).open(dir.join("fxr_hunter.log")) {
        let _ = LOG.set(Mutex::new(f));
    }
    match load_ids() {
        Ok(n) => log(&format!("{n} FXR IDs loaded")),
        Err(e) => {
            log(&format!("not active: {e}"));
            return;
        }
    }
    settings::load();
    if let Err(e) = install_get_hook() {
        log(&format!("not active: {e}"));
        return;
    }
    match freeze::install() {
        Ok(()) => log("freeze patch installed"),
        Err(e) => log(&format!("freeze unavailable: {e}")),
    }
    std::thread::spawn(ticker);
    std::thread::spawn(input::input_loop);
    std::thread::spawn(server::serve);
    std::thread::spawn(launch_ui);
    log("ready: F9 record, F10 stop, F11 freeze, Up/Down select, Ins color, Del hide, Home solo, PgUp watch, PgDn reset");
}

/// Starts the UI (fxr_hunter_ui.exe next to this DLL), tied to the game: it closes when the game exits.
fn launch_ui() {
    let Some(exe) = dll_dir().map(|d| d.join(UI_EXE)) else { return };
    if !exe.exists() {
        log(&format!("no UI: {} not found", exe.display()));
        return;
    }
    match std::process::Command::new(&exe).spawn() {
        Ok(child) => {
            let tied = win::kill_with_this_process(&child);
            log(&format!("UI started (pid {}){}", child.id(), if tied { "" } else { ", but not tied to the game" }));
        }
        Err(e) => log(&format!("UI failed to start: {e}")),
    }
}

#[unsafe(no_mangle)]
extern "system" fn DllMain(module: *mut u8, reason: u32, _reserved: *mut u8) -> i32 {
    const DLL_PROCESS_ATTACH: u32 = 1;
    if reason == DLL_PROCESS_ATTACH {
        MODULE.store(module, Ordering::Relaxed);
        init();
    }
    1
}
