//! Hotkeys, polled while the game window is focused: F9 record, F10 stop recording, F11 slow on/off. Everything else
//! is in the UI (mouse).

use std::time::Duration;

use crate::session;
use crate::win::{game_focused, key_down};

const VK_F9: i32 = 0x78;
const VK_F10: i32 = 0x79;
const VK_F11: i32 = 0x7A;

pub fn input_loop() {
    let keys: [(i32, fn()); 3] = [(VK_F9, session::record), (VK_F10, session::stop), (VK_F11, session::toggle_freeze)];
    let mut down = [false; 3];
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
