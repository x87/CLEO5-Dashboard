//! Helpers shared by the menu: CLEO and game paths, text elision, and the hotkey's key
//! state. None of them know about the render loop.

use hudhook::windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use imgui::Ui;

use crate::sdk::CLEO_GetCleoDirectory;

pub(crate) fn ptr_to_rust_str<'a>(ptr: *const i8) -> &'a str {
    unsafe {
        assert!(!ptr.is_null());
        std::ffi::CStr::from_ptr(ptr).to_str().unwrap()
    }
}

/// CLEO's directory, where `.cleo_config.ini` and the logs live.
pub(crate) fn cleo_dir() -> String {
    let cleo_dir = unsafe { CLEO_GetCleoDirectory() };
    if cleo_dir.is_null() {
        ".".to_string()
    } else {
        ptr_to_rust_str(cleo_dir).to_string()
    }
}

/// The `[Plugins]` section of CLEO's config, which sits next to CLEO.asi.
pub(crate) fn settings_file_path() -> std::path::PathBuf {
    std::path::Path::new(&cleo_dir()).join(".cleo_config.ini")
}

pub(crate) fn make_path_relative_to_exe(path: &str) -> String {
    if let Ok(current_exe_path) = std::env::current_exe() {
        if let (Ok(canonical_exe_dir), Ok(canonical_stream_path)) = (
            current_exe_path.parent().unwrap().canonicalize(),
            std::path::Path::new(&path).canonicalize(),
        ) {
            if let Ok(relative_path) = canonical_stream_path.strip_prefix(&canonical_exe_dir) {
                return relative_path.to_str().unwrap_or(path).to_string();
            }
        }
    }

    return path.to_string();
}

/// Shorten a script path to `budget` pixels, keeping its head and its tail.
///
/// Script paths repeat their middle (`modloader\<mod>\cleo\<file>`), so dropping it keeps
/// both the mod that owns the file and the file name, which is what tells two scripts
/// apart. As many trailing folders as fit are kept.
pub(crate) fn elide_path(ui: &Ui, path: &str, budget: f32) -> String {
    use std::path::MAIN_SEPARATOR as SEP;

    let fits = |text: &str| ui.calc_text_size(text)[0] <= budget;
    if fits(path) {
        return path.to_string();
    }

    let parts: Vec<&str> = path.split(SEP).collect();
    if parts.len() < 2 {
        return elide_tail(ui, path, budget);
    }

    let head = format!("{}{}...{}", parts[0], SEP, SEP);
    let mut tail = String::new();
    for part in parts[1..].iter().rev() {
        let candidate = if tail.is_empty() {
            (*part).to_string()
        } else {
            format!("{part}{SEP}{tail}")
        };
        // Always keep the file name; if even that overflows, cut its head.
        if tail.is_empty() || fits(&format!("{head}{candidate}")) {
            tail = candidate;
        } else {
            break;
        }
    }

    let shortened = format!("{head}{tail}");
    if fits(&shortened) {
        shortened
    } else {
        elide_tail(ui, &shortened, budget)
    }
}

/// Drop characters from the front, keeping a leading `...`.
fn elide_tail(ui: &Ui, text: &str, budget: f32) -> String {
    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() {
        chars.remove(0);
        let candidate: String = format!("...{}", chars.iter().collect::<String>());
        if ui.calc_text_size(&candidate)[0] <= budget {
            return candidate;
        }
    }
    String::new()
}

pub(crate) fn async_key_down(key: i32) -> bool {
    (unsafe { GetAsyncKeyState(key) }) as u16 & 0x8000 != 0
}
