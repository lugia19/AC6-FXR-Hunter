//! Game time scaling (freeze).
//!
//! The flipper (CSFlipperImp) computes the frame's delta time once per frame and stores it at `+0x268`; everything
//! simulated (effects, physics, animation) runs on it. Same mechanism as vswarte's ac6-freecam time control, which
//! hooked the whole update function (0x14175a7b0 on 1.06.1). On 1.09.1 its entry sits behind Arxan stubs, so this
//! patches the final store instead:
//!
//!   0x14175fb80  F3 0F 11 8B 68 02 00 00   movss [rbx+0x268], xmm1    ; clamped delta time
//!
//! becomes a jump to a stub that does `mulss xmm1, [scale]`, the original store, and jumps back to 0x14175fb88.
//! xmm1 is also used right after (a timer at +0x27c counts down by it), which then runs on the scaled time too.

use std::sync::atomic::{AtomicPtr, AtomicU32, AtomicU64, Ordering};

use crate::win::*;

const STORE_ADDR: usize = 0x14175fb80;
const STORE_BYTES: [u8; 8] = [0xF3, 0x0F, 0x11, 0x8B, 0x68, 0x02, 0x00, 0x00];
const SCALE_OFFSET: usize = 0x40;

static SCALE: AtomicPtr<AtomicU32> = AtomicPtr::new(std::ptr::null_mut());

/// Allocates a page within +-2 GB of `target` (for rel32 jumps), searching downwards then upwards.
unsafe fn alloc_near(target: usize) -> Option<*mut u8> {
    const STEP: usize = 0x10000;
    let base = target & !(STEP - 1);
    for i in 1..0x7000usize {
        for addr in [base.wrapping_sub(i * STEP), base + i * STEP] {
            let p = unsafe { VirtualAlloc(addr as *mut u8, 0x1000, MEM_COMMIT | MEM_RESERVE, PAGE_EXECUTE_READWRITE) };
            if !p.is_null() {
                return Some(p);
            }
        }
    }
    None
}

fn rel32(from_next: usize, to: usize) -> [u8; 4] {
    ((to as isize - from_next as isize) as i32).to_le_bytes()
}

pub fn install() -> Result<(), String> {
    let target = STORE_ADDR as *mut u8;
    let current = unsafe { std::slice::from_raw_parts(target, 8) };
    if current != STORE_BYTES {
        return Err(format!("unexpected bytes at {STORE_ADDR:#x}: {current:02x?} (not 1.09.1?)"));
    }
    let stub = unsafe { alloc_near(STORE_ADDR) }.ok_or("no memory near the flipper code")?;
    let s = stub as usize;
    let mut code = Vec::new();
    // mulss xmm1, [rip + disp32]   (F3 0F 59 0D disp32), 8 bytes
    code.extend_from_slice(&[0xF3, 0x0F, 0x59, 0x0D]);
    code.extend_from_slice(&rel32(s + 8, s + SCALE_OFFSET));
    // the original store
    code.extend_from_slice(&STORE_BYTES);
    // jmp back
    code.push(0xE9);
    code.extend_from_slice(&rel32(s + code.len() + 4, STORE_ADDR + 8));
    unsafe {
        std::ptr::copy_nonoverlapping(code.as_ptr(), stub, code.len());
        let scale = stub.add(SCALE_OFFSET) as *mut AtomicU32;
        (*scale).store(1.0f32.to_bits(), Ordering::SeqCst);
        SCALE.store(scale, Ordering::SeqCst);

        let mut patch = vec![0xE9];
        patch.extend_from_slice(&rel32(STORE_ADDR + 5, s));
        patch.extend_from_slice(&[0x90, 0x90, 0x90]);
        let mut old = 0;
        if VirtualProtect(target, 8, PAGE_EXECUTE_READWRITE, &mut old) == 0 {
            return Err("VirtualProtect failed".into());
        }
        // One aligned 8-byte write (0x14175fb80 is 16-aligned), so no thread can run half a patch.
        (*(target as *const AtomicU64)).store(u64::from_le_bytes(patch.try_into().unwrap()), Ordering::SeqCst);
        VirtualProtect(target, 8, old, &mut old);
        FlushInstructionCache(GetCurrentProcess(), target, 8);
    }
    Ok(())
}

pub fn set_scale(scale: f32) {
    let p = SCALE.load(Ordering::SeqCst);
    if !p.is_null() {
        unsafe { (*p).store(scale.to_bits(), Ordering::SeqCst) };
    }
}

pub fn scale() -> f32 {
    let p = SCALE.load(Ordering::SeqCst);
    if p.is_null() { 1.0 } else { f32::from_bits(unsafe { (*p).load(Ordering::SeqCst) }) }
}

pub fn available() -> bool {
    !SCALE.load(Ordering::SeqCst).is_null()
}
