//! The DLL entry point and the hook installation, spelled out from `hudhook!` so every step is
//! logged.

use std::ffi::c_void;
use std::panic::AssertUnwindSafe;

use hudhook::hooks::dx9::ImguiDx9Hooks;
use hudhook::windows::Win32::Foundation::HINSTANCE;
use hudhook::windows::Win32::Graphics::Direct3D9::{
    Direct3DCreate9, D3DADAPTER_DEFAULT, D3DADAPTER_IDENTIFIER9, D3DCAPS9,
    D3DDEVCAPS_HWTRANSFORMANDLIGHT, D3DDEVTYPE_HAL, D3DDISPLAYMODE, D3D_SDK_VERSION,
};
use hudhook::windows::Win32::System::SystemServices::{DLL_PROCESS_ATTACH, DLL_PROCESS_DETACH};
use hudhook::Hudhook;

use crate::app::Dashboard;
use crate::dashboard_log;
use crate::debug;
use crate::hook_probe;
use crate::overlay;

#[allow(non_snake_case)]
#[no_mangle]
pub unsafe extern "system" fn DllMain(hmodule: HINSTANCE, reason: u32, _reserved: *mut c_void) {
    if reason == DLL_PROCESS_DETACH {
        if overlay::is_open() {
            overlay::set_open(false);
        }
        debug::unregister();
        return;
    }

    if reason != DLL_PROCESS_ATTACH {
        return;
    }

    dashboard_log::install();

    let module = hmodule.0 as usize;
    log::info!("DllMain: process attach, module at {module:#010X}");

    std::thread::spawn(move || {
        let module = HINSTANCE(module as *mut c_void);
        let installed = std::panic::catch_unwind(AssertUnwindSafe(move || install_hooks(module)));
        if let Err(payload) = installed {
            log::error!(
                "hook install: the install thread panicked: {} - the overlay will not appear",
                dashboard_log::panic_message(&*payload)
            );
        }
    });
}

fn install_hooks(module: HINSTANCE) {
    log::info!("hook install: thread running");

    // Only from this thread: `CLEO_Log` takes CLEO's lock and `DllMain` holds the loader lock.
    dashboard_log::enable_cleo_mirror();
    dashboard_log::configure();

    // Before hudhook installs anything, or our own detour shows up as someone else's.
    hook_probe::probe();

    log_device();

    match Hudhook::builder()
        .with::<ImguiDx9Hooks>(Dashboard::default())
        .with_hmodule(module)
        .build()
        .apply()
    {
        Ok(()) => log::info!(
            "hook install: Present and Reset detoured; the overlay draws on the next frame"
        ),
        // MinHook is linked into this plugin, so its hook table is ours alone.
        Err(status) => log::error!(
            "hook install: enabling the hooks failed ({status:?}); hudhook ejected itself, so the \
             overlay will never appear"
        ),
    }
}

/// Logs the `device:` line: adapter, driver and vertex processing.
fn log_device() {
    let Some(d3d) = (unsafe { Direct3DCreate9(D3D_SDK_VERSION) }) else {
        log::error!("device: Direct3DCreate9 failed; d3d9.dll is missing or refused to load");
        return;
    };

    let mut mode = D3DDISPLAYMODE::default();
    if let Err(error) = unsafe { d3d.GetAdapterDisplayMode(D3DADAPTER_DEFAULT, &mut mode) } {
        log::warn!("device: GetAdapterDisplayMode failed: {error:?}");
    }

    let mut identifier: D3DADAPTER_IDENTIFIER9 = unsafe { std::mem::zeroed() };
    let adapter = match unsafe { d3d.GetAdapterIdentifier(D3DADAPTER_DEFAULT, 0, &mut identifier) }
    {
        Ok(()) => format!(
            "{} [{:04X}:{:04X}] driver {}",
            fixed_string(&identifier.Description),
            identifier.VendorId,
            identifier.DeviceId,
            fixed_string(&identifier.Driver),
        ),
        Err(error) => format!("<GetAdapterIdentifier failed: {error:?}>"),
    };

    let mut caps: D3DCAPS9 = unsafe { std::mem::zeroed() };
    let pipeline = match unsafe { d3d.GetDeviceCaps(D3DADAPTER_DEFAULT, D3DDEVTYPE_HAL, &mut caps) }
    {
        Ok(()) if caps.DevCaps & D3DDEVCAPS_HWTRANSFORMANDLIGHT as u32 != 0 => {
            "HAL, hardware vertex processing".to_string()
        }
        Ok(()) => "HAL, software vertex processing".to_string(),
        Err(error) => format!("<GetDeviceCaps failed: {error:?}>"),
    };

    log::info!(
        "device: {adapter} | {pipeline} | {}x{}@{}, format 0x{:X}",
        mode.Width,
        mode.Height,
        mode.RefreshRate,
        mode.Format.0,
    );
}

/// A fixed-size C string field, as the D3D9 structs spell them.
fn fixed_string(bytes: &[i8]) -> String {
    let used = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    let raw: Vec<u8> = bytes[..used].iter().map(|byte| *byte as u8).collect();
    String::from_utf8_lossy(&raw).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::fixed_string;

    #[test]
    fn a_fixed_string_stops_at_the_terminator() {
        let field = [b'a' as i8, b'b' as i8, 0, b'c' as i8];
        assert_eq!(fixed_string(&field), "ab");
    }

    #[test]
    fn an_unterminated_fixed_string_uses_the_whole_field() {
        let field = [b'x' as i8; 4];
        assert_eq!(fixed_string(&field), "xxxx");
    }
}
