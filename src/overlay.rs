//! Cursor release and game input freeze.
//!
//! The window procedure belongs to hudhook, so messages are only observed. The OS
//! cursor is freed by redirecting the game's own `ClipCursor`/`SetCursorPos` imports,
//! and mouse look is stopped by detouring the pad's mouse poll.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

use hudhook::windows::core::{BOOL, PCWSTR};
use hudhook::windows::Win32::Foundation::RECT;
use hudhook::windows::Win32::System::LibraryLoader::GetModuleHandleW;
use hudhook::windows::Win32::System::Memory::{
    VirtualProtect, PAGE_PROTECTION_FLAGS, PAGE_READWRITE,
};

/// GTA SA 1.0 US: the three 20-byte mouse states of `CPad` (flags, then X at +0xC,
/// Y at +0x10).
const MOUSE_STATES: [usize; 3] = [0xB73404, 0xB73418, 0xB7342C];
const MOUSE_STATE_SIZE: usize = 0x14;

/// GTA SA 1.0 US: mouse poll inside `CPad::UpdatePads` (thiscall, ecx = state).
const CPAD_MOUSE_POLL: usize = 0x53F3C0;

pub static IS_OPEN: AtomicBool = AtomicBool::new(false);
/// The `Release mouse cursor` setting.
pub static RELEASE_CURSOR: AtomicBool = AtomicBool::new(true);
/// The `Freeze game input` setting.
pub static FREEZE_INPUT: AtomicBool = AtomicBool::new(true);

type ClipCursorFn = unsafe extern "system" fn(*const RECT) -> BOOL;
type SetCursorPosFn = unsafe extern "system" fn(i32, i32) -> BOOL;

static ORIG_CLIP_CURSOR: AtomicUsize = AtomicUsize::new(0);
static ORIG_SET_CURSOR_POS: AtomicUsize = AtomicUsize::new(0);
static SAVED_CLIP: Mutex<Option<RECT>> = Mutex::new(None);

pub fn is_open() -> bool {
    IS_OPEN.load(Ordering::Relaxed)
}

pub fn set_open(open: bool) {
    let was_open = IS_OPEN.swap(open, Ordering::Relaxed);
    if was_open == open {
        return;
    }

    unsafe {
        if !open {
            // Let the game re-clip next frame rather than restoring a stale rect.
            let _ = call_clip_cursor(std::ptr::null());
            if let Ok(mut saved) = SAVED_CLIP.lock() {
                *saved = None;
            }
        }
    }
}

unsafe extern "system" fn clip_cursor_detour(rect: *const RECT) -> BOOL {
    if is_open() && RELEASE_CURSOR.load(Ordering::Relaxed) {
        if !rect.is_null() {
            if let Ok(mut saved) = SAVED_CLIP.lock() {
                *saved = Some(*rect);
            }
        }

        // ImGui needs an unclipped cursor to travel across the whole window.
        return call_clip_cursor(std::ptr::null());
    }

    call_clip_cursor(rect)
}

unsafe extern "system" fn set_cursor_pos_detour(x: i32, y: i32) -> BOOL {
    if is_open() && RELEASE_CURSOR.load(Ordering::Relaxed) {
        // The game re-centres the cursor every frame for mouse look; swallow it.
        return BOOL(1);
    }

    call_set_cursor_pos(x, y)
}

unsafe fn call_clip_cursor(rect: *const RECT) -> BOOL {
    let addr = ORIG_CLIP_CURSOR.load(Ordering::Relaxed);
    if addr == 0 {
        return BOOL(1);
    }

    let orig: ClipCursorFn = std::mem::transmute(addr);
    orig(rect)
}

unsafe fn call_set_cursor_pos(x: i32, y: i32) -> BOOL {
    let addr = ORIG_SET_CURSOR_POS.load(Ordering::Relaxed);
    if addr == 0 {
        return BOOL(1);
    }

    let orig: SetCursorPosFn = std::mem::transmute(addr);
    orig(x, y)
}

/// Stop the camera drifting behind the menu.
///
/// # Safety
/// Writes into the game's `CPad` state; GTA SA 1.0 US only.
pub unsafe fn zero_mouse_delta() {
    for state in MOUSE_STATES {
        std::ptr::write_bytes(state as *mut u8, 0, MOUSE_STATE_SIZE);
    }
}

static ORIG_MOUSE_POLL: AtomicUsize = AtomicUsize::new(0);

/// Skipped entirely while the menu is open: SA reads DirectInput, so filtering window
/// messages is not enough to stop the game seeing movement.
unsafe extern "C" fn mouse_poll_detour() {
    if is_open() && FREEZE_INPUT.load(Ordering::Relaxed) {
        zero_mouse_delta();
        return;
    }

    let original = ORIG_MOUSE_POLL.load(Ordering::Relaxed);
    if original != 0 {
        let poll: unsafe extern "C" fn() = std::mem::transmute(original);
        poll();
    }
}

pub fn install_input_freeze_hook() -> bool {
    unsafe {
        // hudhook already initialised MinHook; MH_ERROR_ALREADY_INITIALIZED is expected.
        let _ = hudhook::mh::MH_Initialize();

        let Ok(hook) = hudhook::mh::MhHook::new(
            CPAD_MOUSE_POLL as *mut c_void,
            mouse_poll_detour as *mut c_void,
        ) else {
            return false;
        };

        ORIG_MOUSE_POLL.store(hook.trampoline() as usize, Ordering::Relaxed);

        if hook.queue_enable().is_err() || hudhook::mh::MH_ApplyQueued().ok().is_err() {
            return false;
        }

        true
    }
}

/// Returns whether both slots were found.
pub fn install_iat_hooks() -> bool {
    unsafe {
        let Ok(module) = GetModuleHandleW(PCWSTR::null()) else {
            return false;
        };

        let base = module.0 as usize;
        let mut hooked = 0;

        let clip_detour: ClipCursorFn = clip_cursor_detour;
        let cursor_detour: SetCursorPosFn = set_cursor_pos_detour;

        if let Some(slot) = find_import_slot(base, "USER32.dll", "ClipCursor") {
            let old = patch_slot(slot, clip_detour as usize);
            if old != clip_detour as usize {
                ORIG_CLIP_CURSOR.store(old, Ordering::Relaxed);
                hooked |= 1;
            }
        }

        if let Some(slot) = find_import_slot(base, "USER32.dll", "SetCursorPos") {
            let old = patch_slot(slot, cursor_detour as usize);
            if old != cursor_detour as usize {
                ORIG_SET_CURSOR_POS.store(old, Ordering::Relaxed);
                hooked |= 2;
            }
        }

        hooked == 3
    }
}

unsafe fn patch_slot(slot: *mut usize, new: usize) -> usize {
    let mut old_prot = PAGE_PROTECTION_FLAGS(0);
    if VirtualProtect(
        slot as *const c_void,
        std::mem::size_of::<usize>(),
        PAGE_READWRITE,
        &mut old_prot,
    )
    .is_err()
    {
        return 0;
    }

    let previous = *slot;
    *slot = new;

    let mut ignored = PAGE_PROTECTION_FLAGS(0);
    let _ = VirtualProtect(
        slot as *const c_void,
        std::mem::size_of::<usize>(),
        old_prot,
        &mut ignored,
    );

    previous
}

/// Walk a module's import directory looking for the IAT entry of `dll!func`.
unsafe fn find_import_slot(base: usize, dll: &str, func: &str) -> Option<*mut usize> {
    if base == 0 || *(base as *const u16) != 0x5A4D {
        return None; // not an MZ header
    }

    let nt = base + *(base.wrapping_add(0x3C) as *const u32) as usize;
    if *(nt as *const u32) != 0x0000_4550 {
        return None; // not a PE header
    }

    let optional = nt + 24;
    let magic = *(optional as *const u16);
    let data_dir = match magic {
        0x10B => optional + 96,  // PE32
        0x20B => optional + 112, // PE32+
        _ => return None,
    };

    let import_rva = *(data_dir.wrapping_add(8) as *const u32) as usize;
    if import_rva == 0 {
        return None;
    }

    let mut descriptor = base + import_rva;
    loop {
        let original_first_thunk = *(descriptor as *const u32) as usize;
        let name_rva = *(descriptor.wrapping_add(12) as *const u32) as usize;
        let first_thunk = *(descriptor.wrapping_add(16) as *const u32) as usize;

        if name_rva == 0 && first_thunk == 0 {
            return None;
        }

        if name_rva != 0 && c_str(base + name_rva).eq_ignore_ascii_case(dll) {
            let thunk = if original_first_thunk != 0 {
                original_first_thunk
            } else {
                first_thunk
            };

            let mut index = 0usize;
            loop {
                let entry =
                    *(base.wrapping_add(thunk).wrapping_add(index * 4) as *const u32) as usize;
                if entry == 0 {
                    break;
                }

                // Ordinal imports have the high bit set; these two are imported by name.
                if entry & 0x8000_0000 == 0 {
                    let name = c_str(base + (entry & 0x7FFF_FFFF) + 2);
                    if name.eq_ignore_ascii_case(func) {
                        return Some(base.wrapping_add(first_thunk).wrapping_add(index * 4)
                            as *mut usize);
                    }
                }

                index += 1;
            }
        }

        descriptor = descriptor.wrapping_add(20); // sizeof(IMAGE_IMPORT_DESCRIPTOR)
    }
}

unsafe fn c_str(address: usize) -> &'static str {
    let mut len = 0usize;
    while *((address + len) as *const u8) != 0 {
        len += 1;
    }

    std::str::from_utf8_unchecked(std::slice::from_raw_parts(address as *const u8, len))
}
