//! Small helpers shared across the menu: CLEO paths, text elision and the hotkey's key
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
    let dir = unsafe { CLEO_GetCleoDirectory() };
    if dir.is_null() {
        ".".to_string()
    } else {
        ptr_to_rust_str(dir).to_string()
    }
}

/// The `[Plugins]` section of CLEO's config, which sits next to CLEO.asi.
pub(crate) fn settings_file_path() -> std::path::PathBuf {
    std::path::Path::new(&cleo_dir()).join(".cleo_config.ini")
}

pub(crate) fn make_path_relative_to_exe(path: &str) -> String {
    let Ok(exe) = std::env::current_exe() else {
        return path.to_string();
    };
    let (Some(exe_dir), Ok(absolute)) = (exe.parent(), std::path::Path::new(path).canonicalize())
    else {
        return path.to_string();
    };
    let Ok(exe_dir) = exe_dir.canonicalize() else {
        return path.to_string();
    };

    match absolute.strip_prefix(&exe_dir) {
        Ok(relative) => relative.to_str().unwrap_or(path).to_string(),
        Err(_) => path.to_string(),
    }
}

/// Shorten a script path to `budget` pixels, keeping its head and its tail.
///
/// Script paths repeat their middle (`modloader\<mod>\cleo\<file>`), so dropping it keeps
/// both the mod that owns the file and its name. As many trailing folders as fit are kept.
pub(crate) fn elide_path(ui: &Ui, path: &str, budget: f32) -> String {
    use std::path::MAIN_SEPARATOR as SEP;

    if fits(ui, path, budget) {
        return path.to_string();
    }

    let parts: Vec<&str> = path.split(SEP).collect();
    if parts.len() < 2 {
        return elide_front(ui, path, budget);
    }

    let head = format!("{}{}...{}", parts[0], SEP, SEP);
    let mut tail = String::new();
    for part in parts[1..].iter().rev() {
        let candidate = if tail.is_empty() {
            (*part).to_string()
        } else {
            format!("{part}{SEP}{tail}")
        };
        // Always keep the file name; if even that overflows, its head is cut below.
        if tail.is_empty() || fits(ui, &format!("{head}{candidate}"), budget) {
            tail = candidate;
        } else {
            break;
        }
    }

    let shortened = format!("{head}{tail}");
    if fits(ui, &shortened, budget) {
        shortened
    } else {
        elide_front(ui, &shortened, budget)
    }
}

/// Truncate the end of `text` with an ellipsis until it fits `budget`: for values that are
/// read from the left, where the beginning says what the thing is.
pub(crate) fn elide_end(ui: &Ui, text: &str, budget: f32) -> String {
    if fits(ui, text, budget) {
        return text.to_string();
    }

    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let candidate: String = chars.iter().copied().chain(std::iter::once('…')).collect();
        if fits(ui, &candidate, budget) {
            return candidate;
        }
    }
    String::new()
}

fn fits(ui: &Ui, text: &str, budget: f32) -> bool {
    ui.calc_text_size(text)[0] <= budget
}

/// Drop characters from the front of `text`, keeping a leading `...`.
fn elide_front(ui: &Ui, text: &str, budget: f32) -> String {
    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() {
        chars.remove(0);
        let candidate: String = format!("...{}", chars.iter().collect::<String>());
        if fits(ui, &candidate, budget) {
            return candidate;
        }
    }
    String::new()
}

/// A pointer or offset as `0x00401234`, the spelling the panels show.
pub(crate) fn hex_address(value: usize) -> String {
    format!("0x{value:08X}")
}

pub(crate) fn async_key_down(key: i32) -> bool {
    (unsafe { GetAsyncKeyState(key) }) as u16 & 0x8000 != 0
}
