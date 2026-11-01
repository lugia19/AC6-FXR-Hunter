//! Local control socket for the UI: 127.0.0.1:48690, one command per line, one JSON state line back per command.
//!
//! Commands: state | record | stop | freeze (toggle) | scale <f32>
//!           | freezescale <f32> (game speed while frozen, F11 and freeze on appear; 0.0001 = stopped)
//!           | look <fxr id> <0-7> | watch <fxr id> (toggle) | hide <fxr id> [0|1] (no value: toggle)
//!           | solo <fxr id...> (hide every other listed FXR)
//!           | unhide | reset | add <fxr id> | remove <fxr id> (from the list; recorded: until the next recording)
//!           | log start <interval ms> | log stop (seen log, seenlog.rs)
//!           | ignore <fxr id> (append to the ignore list) | ignore reload
//!           | settings ignore <0|1> | settings range <0|1> <min> <max>
//! FXR IDs can be any injected FXR, recorded or not. Looks: 0 stock, 2 red, 3 green, 4 blue, 5 yellow, 6 cyan,
//! 7 magenta (1 = hidden, same as hide <id> 1).

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};

use crate::{freeze, log, seenlog, session, settings};

pub const PORT: u16 = 48690;

fn run(line: &str) -> Result<(), String> {
    let mut parts = line.split_whitespace();
    let cmd = parts.next().unwrap_or("state");
    let mut id = || -> Result<i32, String> {
        parts.next().and_then(|p| p.parse().ok()).ok_or_else(|| format!("usage: {cmd} <fxr id>"))
    };
    let not_injected = |fxr: i32| format!("FXR {fxr} isn't injected");
    match cmd {
        "state" => {}
        "record" => session::record(),
        "stop" => session::stop(),
        "freeze" => session::toggle_freeze(),
        "freezescale" => session::set_freeze_scale(parts.next().and_then(|p| p.parse().ok()).ok_or("usage: freezescale <f32>")?),
        "scale" => freeze::set_scale(parts.next().and_then(|p| p.parse().ok()).ok_or("usage: scale <f32>")?),
        "unhide" => session::unhide_all(),
        "reset" => session::reset_looks(),
        "ignore" => match parts.next() {
            Some("reload") => settings::reload_ignore(),
            Some(p) => settings::ignore(p.parse().map_err(|_| "usage: ignore <fxr id> | ignore reload".to_string())?)?,
            None => return Err("usage: ignore <fxr id> | ignore reload".into()),
        },
        "settings" => {
            let flag = |p: Option<&str>| match p {
                Some("1") => Ok(true),
                Some("0") => Ok(false),
                _ => Err("usage: settings ignore <0|1> | settings range <0|1> <min> <max>".to_string()),
            };
            match parts.next() {
                Some("ignore") => settings::set_ignore_enabled(flag(parts.next())?),
                Some("range") => {
                    let on = flag(parts.next())?;
                    let mut num = || parts.next().and_then(|p| p.parse::<i32>().ok()).ok_or("usage: settings range <0|1> <min> <max>");
                    let (min, max) = (num()?, num()?);
                    settings::set_range(on, min, max)
                }
                _ => return Err("usage: settings ignore <0|1> | settings range <0|1> <min> <max>".into()),
            }
        }
        "log" => match parts.next() {
            Some("start") => seenlog::start(parts.next().and_then(|p| p.parse().ok()).ok_or("usage: log start <interval ms>")?)?,
            Some("stop") => seenlog::stop(),
            _ => return Err("usage: log start <interval ms> | log stop".into()),
        },
        "add" => {
            let fxr = id()?;
            session::add(fxr).then_some(()).ok_or_else(|| not_injected(fxr))?
        }
        "remove" => {
            let fxr = id()?;
            session::remove(fxr).then_some(()).ok_or_else(|| not_injected(fxr))?
        }
        "solo" => {
            let ids: Vec<i32> = parts.by_ref().map(|p| p.parse()).collect::<Result<_, _>>().map_err(|_| "usage: solo <fxr id...>")?;
            if ids.is_empty() {
                return Err("usage: solo <fxr id...>".into());
            }
            session::solo(&ids).then_some(()).ok_or("an FXR isn't injected")?
        }
        "hide" => {
            let fxr = id()?;
            let hide = match parts.next() {
                None => None,
                Some("1") => Some(true),
                Some("0") => Some(false),
                Some(_) => return Err("usage: hide <fxr id> [0|1]".into()),
            };
            session::set_hidden(fxr, hide).then_some(()).ok_or_else(|| not_injected(fxr))?
        }
        "watch" => {
            let fxr = id()?;
            session::toggle_watch(fxr).then_some(()).ok_or_else(|| not_injected(fxr))?
        }
        "look" => {
            let fxr = id()?;
            let mode: u8 = parts.next().and_then(|p| p.parse().ok()).filter(|&m| m <= 7).ok_or("usage: look <fxr id> <0-7>")?;
            session::set_look(fxr, mode).then_some(()).ok_or_else(|| not_injected(fxr))?
        }
        other => return Err(format!("unknown command {other}")),
    }
    Ok(())
}

fn handle(stream: TcpStream) {
    let Ok(mut writer) = stream.try_clone() else { return };
    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else { return };
        let result = run(&line);
        let mut state = session::state();
        state["log"] = seenlog::state();
        state["settings"] = settings::state();
        if let Err(e) = result {
            state["error"] = e.into();
        }
        if writeln!(writer, "{state}").is_err() {
            return;
        }
    }
}

pub fn serve() {
    let listener = match TcpListener::bind(("127.0.0.1", PORT)) {
        Ok(l) => l,
        Err(e) => {
            log(&format!("control socket unavailable: {e}"));
            return;
        }
    };
    log(&format!("control socket on 127.0.0.1:{PORT}"));
    for stream in listener.incoming().flatten() {
        std::thread::spawn(move || handle(stream));
    }
}
