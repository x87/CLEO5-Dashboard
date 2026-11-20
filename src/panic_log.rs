//! A place for a panic to be seen.
//!
//! The plugin runs inside a GUI process: a panic's message goes to a stderr nobody owns, and
//! the unwind behind it reaches an `extern "system"` boundary - hudhook's detour, or CLEO's
//! opcode callback - where Rust has to abort the whole process. The user sees the C runtime's
//! "This application has requested the Runtime to terminate it in an unusual way" box and
//! nothing else: no stack, no message, no exit code worth reading.
//!
//! [`install`] writes the message, its source location and a backtrace to a file beside the
//! game before that happens, and the two boundaries in this crate catch the unwind so the
//! game survives it: a frame is skipped, not the session.

use std::backtrace::Backtrace;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Once;

use hudhook::windows::Win32::System::SystemInformation::GetLocalTime;

static INSTALLED: Once = Once::new();

/// Where the hook writes: beside `gta_sa.exe`, which is the directory the plugin's own
/// configuration sits under and the first place anyone looks.
pub fn log_path() -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.join(FILE))
}

const FILE: &str = "SA.Dashboard.panic.log";

/// Install the hook once. Called from the hook's own initialisation, so a panic while the
/// overlay is being set up is recorded too.
pub fn install() {
    INSTALLED.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let location = info
                .location()
                .map(|location| location.to_string())
                .unwrap_or_else(|| "unknown location".to_string());
            let payload = info.payload();
            let message = payload
                .downcast_ref::<&str>()
                .map(|text| (*text).to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "<panic payload was not a string>".to_string());
            // `force_capture` rather than `capture`: the game has no `RUST_BACKTRACE` to set,
            // and a backtrace is the whole point of the file.
            let text = format!(
                "{}panic at {location}\n{message}\n{}\n",
                stamp(),
                Backtrace::force_capture(),
            );
            if let Some(path) = log_path() {
                if let Ok(mut file) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                {
                    let _ = file.write_all(text.as_bytes());
                    // The process may be about to die: nothing may be left in a buffer.
                    let _ = file.flush();
                }
            }
            // The default hook still runs, so anything watching stderr sees it as before.
            previous(info);
        }));
    });
}

/// `2026-11-19 23:42:02.317 ` - local time, because the file is read next to a wall clock.
fn stamp() -> String {
    let time = unsafe { GetLocalTime() };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03} ",
        time.wYear,
        time.wMonth,
        time.wDay,
        time.wHour,
        time.wMinute,
        time.wSecond,
        time.wMilliseconds,
    )
}
