//! Hotkeys, polled while the game window is focused. Only F-keys, arrows and the Insert/Delete block: the player
//! keeps playing while recording, so nothing that's usually bound to movement or actions.
//!
//! F9 record, F10 stop, F11 freeze/unfreeze, Up/Down select a recorded FXR, Insert cycle its color, Delete hide/show
//! it, Home solo it (hide the other recorded FXRs), Page Up freeze when it appears,
//! Page Down reset all looks.

use std::time::Duration;

use crate::session;
use crate::win::{game_focused, key_down};

const VK_PRIOR: i32 = 0x21; // Page Up
const VK_NEXT: i32 = 0x22; // Page Down
const VK_HOME: i32 = 0x24;
const VK_UP: i32 = 0x26;
const VK_DOWN: i32 = 0x28;
const VK_INSERT: i32 = 0x2D;
const VK_DELETE: i32 = 0x2E;
const VK_F9: i32 = 0x78;
const VK_F10: i32 = 0x79;
const VK_F11: i32 = 0x7A;

pub fn input_loop() {
    let keys: [(i32, fn()); 10] = [
        (VK_F9, session::record),
        (VK_F10, session::stop),
        (VK_F11, session::toggle_freeze),
        (VK_UP, || session::step(-1)),
        (VK_DOWN, || session::step(1)),
        (VK_INSERT, session::cycle_selected),
        (VK_DELETE, session::hide_selected),
        (VK_HOME, session::solo_selected),
        (VK_PRIOR, session::toggle_watch_selected),
        (VK_NEXT, session::reset_looks),
    ];
    let mut down = [false; 10];
    loop {
        std::thread::sleep(Duration::from_millis(10));
        let focused = game_focused();
        for (k, (vk, action)) in keys.iter().enumerate() {
            let now_down = focused && key_down(*vk);
            if now_down && !down[k] {
                action();
            }
            down[k] = now_down;
        }
    }
}
