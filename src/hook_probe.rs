//! What the hook targets look like before this plugin touches them: `d3d9.dll`'s origin, the
//! prologue at `Present`/`Reset` with its detour target resolved to a module, and the modules that
//! import MinHook. Read-only - it creates no hooks.

use hudhook::hooks::DummyHwnd;
use hudhook::windows::core::{Interface, BOOL};
use hudhook::windows::Win32::Graphics::Direct3D9::{
    Direct3DCreate9, D3DADAPTER_DEFAULT, D3DCREATE_SOFTWARE_VERTEXPROCESSING, D3DDEVTYPE_NULLREF,
    D3DDISPLAYMODE, D3DPRESENT_PARAMETERS, D3DSWAPEFFECT_DISCARD, D3D_SDK_VERSION,
};

use crate::modules;
use crate::overlay;

/// Prologue bytes to read; the longest pattern handled is seven.
const PROLOGUE: usize = 16;

pub fn probe() {
    log_d3d9_module();

    if let Some((present, reset)) = device_pointers() {
        log_target("Present", present);
        log_target("Reset", reset);

        if present == reset {
            log::warn!(
                "probe: Present and Reset resolve to the same address; hudhook will fail to create \
                 the second hook (that, not another mod, is what MH_ERROR_ALREADY_CREATED means \
                 here - MinHook is linked into this plugin, not shared)"
            );
        }
    }

    log_minhook_importers();
}

/// Whether `d3d9.dll` is the system's or a wrapper shipped next to the game.
fn log_d3d9_module() {
    let modules = modules::loaded();
    let Some(d3d9) = modules.iter().find(|module| {
        module.path_lower == "d3d9.dll" || module.path_lower.ends_with("\\d3d9.dll")
    }) else {
        log::warn!("probe: d3d9.dll is not loaded at all");
        return;
    };

    if d3d9.system {
        log::info!(
            "probe: d3d9.dll is the system one ({}), base {:#010X}",
            d3d9.path,
            d3d9.base
        );
    } else {
        log::warn!(
            "probe: d3d9.dll is running from the game directory ({}), base {:#010X} - a wrapper \
             (ENB, ReShade, DXVK, dgVoodoo) owns the device, and hudhook detours whatever that \
             exposes",
            d3d9.path,
            d3d9.base
        );
    }
}

/// `Present`/`Reset` from a throwaway NULLREF device, the way hudhook takes them.
fn device_pointers() -> Option<(usize, usize)> {
    let Some(d3d) = (unsafe { Direct3DCreate9(D3D_SDK_VERSION) }) else {
        log::error!("probe: Direct3DCreate9 failed; hudhook cannot find Present either");
        return None;
    };

    let mut mode = D3DDISPLAYMODE::default();
    if let Err(error) = unsafe { d3d.GetAdapterDisplayMode(D3DADAPTER_DEFAULT, &mut mode) } {
        log::warn!("probe: GetAdapterDisplayMode failed: {error:?}");
    }

    let mut parameters = D3DPRESENT_PARAMETERS {
        Windowed: BOOL(1),
        SwapEffect: D3DSWAPEFFECT_DISCARD,
        BackBufferFormat: mode.Format,
        ..Default::default()
    };

    let window = DummyHwnd::new();
    let mut device = None;
    match unsafe {
        d3d.CreateDevice(
            D3DADAPTER_DEFAULT,
            D3DDEVTYPE_NULLREF,
            window.hwnd(),
            D3DCREATE_SOFTWARE_VERTEXPROCESSING as u32,
            &mut parameters,
            &mut device,
        )
    } {
        Ok(()) => {}
        Err(error) => {
            log::error!(
                "probe: the NULLREF device hudhook needs could not be created ({error:?}); hudhook \
                 will panic on this same call and the overlay will not appear"
            );
            return None;
        }
    }

    let device = device?;
    let vtable = Interface::vtable(&device);
    Some((vtable.Present as usize, vtable.Reset as usize))
}

fn log_target(name: &str, address: usize) {
    let prologue = unsafe { std::slice::from_raw_parts(address as *const u8, PROLOGUE) };
    let hex = prologue
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(" ");

    match decode_detour(address, prologue) {
        Some((target, pattern)) => log::warn!(
            "probe: {name} at {address:#010X} is already detoured ({pattern}): {hex} -> \
             {target:#010X}{}",
            owner(target)
        ),
        None => log::info!("probe: {name} at {address:#010X} is not detoured yet: {hex}"),
    }
}

/// The detour a hooked prologue jumps to, and which pattern it is.
fn decode_detour(address: usize, bytes: &[u8]) -> Option<(usize, &'static str)> {
    let u32_at = |offset: usize| -> Option<u32> {
        bytes
            .get(offset..offset + 4)
            .map(|slice| u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
    };
    let relative = |end: usize| -> Option<usize> {
        let displacement = u32_at(1)? as i32 as isize;
        Some((address as isize).wrapping_add(end as isize + displacement) as usize)
    };

    match *bytes.first()? {
        0xE9 => relative(5).map(|target| (target, "jmp rel32")),
        0xEB => {
            let displacement = *bytes.get(1)? as i8 as isize;
            Some((
                (address as isize).wrapping_add(2 + displacement) as usize,
                "jmp rel8",
            ))
        }
        0x68 if bytes.get(5) == Some(&0xC3) => {
            u32_at(1).map(|target| (target as usize, "push imm32; ret"))
        }
        0xFF if bytes.get(1) == Some(&0x25) => {
            let slot = u32_at(2)? as usize;
            let target = unsafe { *(slot as *const u32) } as usize;
            Some((target, "jmp [imm32]"))
        }
        0xB8 if bytes.get(5) == Some(&0xFF) && bytes.get(6) == Some(&0xE0) => {
            u32_at(1).map(|target| (target as usize, "mov eax; jmp eax"))
        }
        _ => None,
    }
}

/// The module a detour target belongs to, as a log-line suffix.
fn owner(target: usize) -> String {
    let modules = modules::loaded();
    for module in modules.iter() {
        let size = image_size(module.base);
        if size > 0 && target >= module.base && target < module.base + size {
            return format!(", inside \"{}\"{:+#X}", module.path, target - module.base);
        }
    }

    ", not inside any loaded module (a hook library's own staging block, or a trampoline)"
        .to_string()
}

fn image_size(base: usize) -> usize {
    if base == 0 || unsafe { *(base as *const u16) } != 0x5A4D {
        return 0; // not an MZ header
    }

    let nt = base + unsafe { *(base.wrapping_add(0x3C) as *const u32) } as usize;
    if unsafe { *(nt as *const u32) } != 0x0000_4550 {
        return 0;
    }

    // `SizeOfImage`: optional header + 56, the same offset for PE32 and PE32+.
    (unsafe { *(nt.wrapping_add(0x50) as *const u32) }) as usize
}

/// Every loaded module that imports MinHook can detour `Present`; this plugin links MinHook
/// statically, so it cannot see anyone else's hook list.
fn log_minhook_importers() {
    let modules = modules::loaded();
    let importers: Vec<&str> = modules
        .iter()
        .filter(|module| !module.system && module.base != 0)
        .filter(|module| {
            overlay::imports_dll(module.base, "MinHook.x86.dll")
                || overlay::imports_dll(module.base, "minhook.dll")
                || overlay::imports_dll(module.base, "minhook.x64.dll")
        })
        .map(|module| module.path.as_str())
        .collect();

    if importers.is_empty() {
        log::info!(
            "probe: no loaded module imports MinHook, so any hook on Present is an inline patch or \
             a wrapper"
        );
    } else {
        log::warn!(
            "probe: modules that import MinHook (each can detour Present): {}",
            importers.join(", ")
        );
    }
}

#[cfg(test)]
mod tests {
    use super::decode_detour;

    const BASE: usize = 0x1000_0000;

    #[test]
    fn a_relative_jump_is_followed() {
        // E9 20 00 00 00 -> base + 5 + 0x20
        let bytes = [0xE9, 0x20, 0x00, 0x00, 0x00, 0x00];
        let (target, pattern) = decode_detour(BASE, &bytes).unwrap();
        assert_eq!(target, BASE + 5 + 0x20);
        assert_eq!(pattern, "jmp rel32");
    }

    #[test]
    fn a_short_jump_is_followed() {
        let bytes = [0xEB, 0x10, 0x00];
        let (target, pattern) = decode_detour(BASE, &bytes).unwrap();
        assert_eq!(target, BASE + 2 + 0x10);
        assert_eq!(pattern, "jmp rel8");
    }

    #[test]
    fn a_push_ret_pair_is_read_as_an_absolute_address() {
        let bytes = [0x68, 0x78, 0x56, 0x34, 0x12, 0xC3];
        let (target, pattern) = decode_detour(BASE, &bytes).unwrap();
        assert_eq!(target, 0x1234_5678);
        assert_eq!(pattern, "push imm32; ret");
    }

    #[test]
    fn an_unpatched_prologue_has_no_target() {
        // push ebp; mov ebp, esp
        let bytes = [0x55, 0x8B, 0xEC, 0x83, 0xEC, 0x10];
        assert!(decode_detour(BASE, &bytes).is_none());
    }

    #[test]
    fn a_truncated_prologue_does_not_read_past_its_end() {
        assert!(decode_detour(BASE, &[0xE9, 0x00]).is_none());
        assert!(decode_detour(BASE, &[]).is_none());
    }

    #[test]
    fn a_mov_eax_jmp_eax_pair_is_followed() {
        let bytes = [0xB8, 0x00, 0x00, 0x00, 0x10, 0xFF, 0xE0];
        let (target, pattern) = decode_detour(BASE, &bytes).unwrap();
        assert_eq!(target, 0x1000_0000);
        assert_eq!(pattern, "mov eax; jmp eax");
    }
}
