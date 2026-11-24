//! The overlay's log.
//!
//! Warnings, errors and startup milestones go to `cleo.log` through [`CLEO_Log`]. Everything else,
//! including backtraces, goes to [`FILE_NAME`], which is off unless [`FILE_LOG_KEY`] is set - a panic
//! opens it regardless.

use std::backtrace::Backtrace;
use std::cell::Cell;
use std::ffi::{CStr, CString};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, Once, OnceLock};

use hudhook::windows::Win32::System::SystemInformation::GetLocalTime;
use log::{Level, LevelFilter, Log, Metadata, Record};

use crate::sdk::{
    CLEO_GetCleoDirectory, CLEO_GetConfigInt, CLEO_GetLogDirectory, CLEO_GetVersionStr, CLEO_Log,
    LOG_DEBUG, LOG_DEFAULT, LOG_ERROR,
};

/// The file this module writes, inside CLEO's log directory.
pub const FILE_NAME: &str = "SA.Dashboard.log";

/// The `[Plugins]` key that turns that file on. Off by default; `1` writes it.
pub const FILE_LOG_KEY: &str = "Dashboard.Debug.Log";

/// Caps the file: a per-frame error at 30 fps would otherwise fill a disk.
const MAX_BYTES: u64 = 1 << 20;

/// Lines held while the file is closed, so enabling it (or a panic) keeps the start of the session.
const PENDING_MAX: usize = 64;

/// Prefixes every line, so a line lifted out of `cleo.log` says where it came from.
const PREFIX: &str = "[Dashboard] ";

static INSTALL: Once = Once::new();
static CONFIGURED: Once = Once::new();
static FILE_OPEN: AtomicBool = AtomicBool::new(false);
static FILE: Mutex<Option<File>> = Mutex::new(None);
static PATH: OnceLock<PathBuf> = OnceLock::new();
/// Held while the file is closed.
static PENDING: Mutex<Vec<String>> = Mutex::new(Vec::new());
static WRITTEN: AtomicU64 = AtomicU64::new(0);
static CAPPED: AtomicBool = AtomicBool::new(false);
/// Off until the install thread runs: `CLEO_Log` takes CLEO's lock, which `DllMain` must not reach.
static MIRROR: AtomicBool = AtomicBool::new(false);
/// The last line handed to `CLEO_Log`, so an identical run only costs one entry.
static STREAK: Mutex<Option<Streak>> = Mutex::new(None);
/// Cached so the panic hook can resolve it from any thread.
static LOG_DIR: OnceLock<PathBuf> = OnceLock::new();

// Set while this thread is inside `CLEO_Log`, which calls every registered `Log` callback - ours
// included, which would otherwise recurse into the mirror.
thread_local! {
    static IN_CLEO: Cell<bool> = const { Cell::new(false) };
}

struct Streak {
    level: Level,
    text: String,
    repeats: u32,
}

/// Wires up `log`, `tracing` and the panic hook. Called from `DllMain`, so it makes no CLEO calls.
pub fn install() {
    INSTALL.call_once(|| {
        // Before the logger exists, so these cannot go through `log`.
        raw(&format!(
            "CLEO5 Dashboard {} starting",
            env!("CARGO_PKG_VERSION")
        ));
        raw(&format!(" exe: {}", exe_path().display()));

        if log::set_logger(&LOGGER).is_err() {
            raw("warning: another logger is installed; log records will not reach these sinks");
        }
        log::set_max_level(LevelFilter::Trace);

        install_panic_hook();

        if tracing::subscriber::set_global_default(Bridge).is_err() {
            raw(
                "warning: another tracing subscriber is installed; hudhook's own messages are lost",
            );
        }
    });
}

/// Reads `Dashboard.Debug.Log` and opens the file when it asks. Called from the install thread,
/// not `DllMain` - it does make CLEO calls.
pub fn configure() {
    CONFIGURED.call_once(|| {
        let key = cs(FILE_LOG_KEY);
        let wanted = unsafe { CLEO_GetConfigInt(key.as_ptr(), 0) } != 0;

        raw(&format!(
            " cleo: {}",
            cleo_string(unsafe { CLEO_GetCleoDirectory() }).unwrap_or_else(|| "?".to_string())
        ));
        raw(&format!(
            " cleo version: {}",
            cleo_string(unsafe { CLEO_GetVersionStr() }).unwrap_or_else(|| "?".to_string())
        ));

        let path = log_dir().join(FILE_NAME);
        if wanted {
            raw(&format!(
                " file log: {} (capped at {} KiB)",
                path.display(),
                MAX_BYTES / 1024
            ));
            open_file(&path);
        } else {
            raw(&format!(
                " file log: off ({FILE_LOG_KEY}=0) - milestones and warnings go to cleo.log; a \
                 panic opens {} anyway",
                path.display()
            ));
        }
    });
}

/// Starts the `cleo.log` mirror. Install thread only, never `DllMain`.
pub fn enable_cleo_mirror() {
    MIRROR.store(true, Ordering::Relaxed);
    log::info!("mirroring this log into cleo.log");
}

/// Whether the overlay's own file is being written this session.
pub fn file_log_enabled() -> bool {
    FILE_OPEN.load(Ordering::Relaxed)
}

/// The file being written, once it has been opened.
pub fn path() -> Option<&'static Path> {
    if !file_log_enabled() {
        return None;
    }

    PATH.get().map(PathBuf::as_path)
}

/// Where this session's lines go, for the startup line and the docs.
pub fn destination() -> String {
    match path() {
        Some(path) => path.display().to_string(),
        None => format!("cleo.log only ({FILE_LOG_KEY}=0)"),
    }
}

// ------------------------------------------------------------------ the sinks

struct Logger;

impl Log for Logger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        let text = format!("{PREFIX}{}", record.args());
        write_line(&format!("{}{:<5} {}", stamp(), record.level(), text), false);
        mirror(record.level(), &text);
    }

    fn flush(&self) {
        if let Ok(mut slot) = FILE.lock() {
            if let Some(file) = slot.as_mut() {
                let _ = file.flush();
            }
        }
    }
}

static LOGGER: Logger = Logger;

/// A line written before `log` is available, or one that must not be capped: panics.
fn raw(text: &str) {
    write_line(&format!("{}{PREFIX}{}", stamp(), text), true);
}

/// Appends a line, or holds it until the file is open. `force` skips the size cap.
fn write_line(line: &str, force: bool) {
    if !FILE_OPEN.load(Ordering::Relaxed) {
        buffer(line);
        return;
    }

    write_now(line, force);
}

fn write_now(line: &str, force: bool) {
    let Ok(mut slot) = FILE.lock() else {
        return;
    };
    let Some(file) = slot.as_mut() else {
        return;
    };

    if !force && WRITTEN.load(Ordering::Relaxed) > MAX_BYTES {
        if !CAPPED.swap(true, Ordering::Relaxed) {
            let _ = writeln!(
                file,
                "{}{PREFIX}----- log capped at {} KiB, later lines are dropped -----",
                stamp(),
                MAX_BYTES / 1024
            );
            let _ = file.flush();
        }
        return;
    }

    let mut text = String::with_capacity(line.len() + 2);
    text.push_str(line);
    text.push('\n');
    if file.write_all(text.as_bytes()).is_err() {
        return;
    }
    let _ = file.flush();
    WRITTEN.fetch_add(text.len() as u64, Ordering::Relaxed);
}

fn buffer(line: &str) {
    if let Ok(mut pending) = PENDING.lock() {
        if pending.len() >= PENDING_MAX {
            pending.remove(0);
        }
        pending.push(line.to_string());
    }
}

/// Opens the file (truncating) and writes what was held while it was closed.
fn open_file(path: &Path) -> bool {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
        .ok();
    if file.is_none() {
        return false;
    }

    if let Ok(mut slot) = FILE.lock() {
        *slot = file;
    }
    let _ = PATH.set(path.to_path_buf());
    FILE_OPEN.store(true, Ordering::Relaxed);
    flush_pending();
    true
}

/// A panic is the one case worth a file even with the setting off.
fn ensure_file() {
    if FILE_OPEN.load(Ordering::Relaxed) {
        return;
    }

    let path = log_dir().join(FILE_NAME);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    // Append, so a crash in a later launch does not eat the earlier report.
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .ok();
    if let Ok(mut slot) = FILE.lock() {
        *slot = file;
    }
    let _ = PATH.set(path);
    FILE_OPEN.store(true, Ordering::Relaxed);
    flush_pending();
}

fn flush_pending() {
    let held = match PENDING.lock() {
        Ok(mut pending) => std::mem::take(&mut *pending),
        Err(_) => return,
    };

    for line in held {
        write_now(&line, true);
    }
}

/// Copy a line into `cleo.log`, collapsing a run of identical ones into a single entry.
fn mirror(level: Level, text: &str) {
    if !MIRROR.load(Ordering::Relaxed) || !mirrored(level) || IN_CLEO.with(Cell::get) {
        return;
    }

    let Ok(mut streak) = STREAK.lock() else {
        return;
    };

    match streak.as_mut() {
        Some(current) if current.level == level && current.text == text => {
            current.repeats += 1;
            return;
        }
        Some(current) if current.repeats > 0 => {
            let note = format!(
                "{PREFIX}(the previous line repeated {} more times)",
                current.repeats
            );
            cleo_line(current.level, &note);
        }
        _ => {}
    }

    cleo_line(level, text);
    *streak = Some(Streak {
        level,
        text: text.to_string(),
        repeats: 0,
    });
}

/// What `cleo.log` gets: errors, warnings and the `Info` milestones. `Debug`/`Trace` stays in the
/// file.
fn mirrored(level: Level) -> bool {
    matches!(level, Level::Error | Level::Warn | Level::Info)
}

fn cleo_line(level: Level, text: &str) {
    let Ok(message) = CString::new(text.replace('\0', "")) else {
        return;
    };

    // CLEO writes the same bytes whatever the level; it only decides who hears the callback.
    let cleo_level = match level {
        Level::Error | Level::Warn => LOG_ERROR,
        Level::Info => LOG_DEFAULT,
        Level::Debug | Level::Trace => LOG_DEBUG,
    };

    IN_CLEO.with(|inside| inside.set(true));
    unsafe { CLEO_Log(cleo_level, message.as_ptr()) };
    IN_CLEO.with(|inside| inside.set(false));
}

// -------------------------------------------------------------------- panics

/// Writes the message, location and backtrace to the file, opening it if it is closed.
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|location| location.to_string())
            .unwrap_or_else(|| "unknown location".to_string());
        let payload = info.payload();
        let message = panic_message(payload);

        ensure_file();

        // `force_capture`: the game has no `RUST_BACKTRACE` to set.
        write_line(
            &format!(
                "{}{PREFIX}----- PANIC at {location}\n{message}\n{}\n",
                stamp(),
                Backtrace::force_capture(),
            ),
            true,
        );
        mirror(
            Level::Error,
            &format!("{PREFIX}panic at {location}: {message}"),
        );

        // The default hook still runs, so stderr sees it as before.
        previous(info);
    }));
}

// ------------------------------------------------------------------- tracing

/// The text of a caught panic payload.
pub fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|text| (*text).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "<panic payload was not a string>".to_string())
}

/// Forwards hudhook's `tracing` events, which otherwise have no subscriber.
struct Bridge;

#[derive(Default)]
struct Message(String);

impl tracing::field::Visit for Message {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.0.push_str(&format!("{value:?}"));
        } else {
            self.0.push_str(&format!(" {}={:?}", field.name(), value));
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.0.push_str(value);
        } else {
            self.0.push_str(&format!(" {}={}", field.name(), value));
        }
    }
}

impl tracing::Subscriber for Bridge {
    fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
        *metadata.level() <= tracing::Level::INFO
    }

    fn new_span(&self, _attributes: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }

    fn record(&self, _span: &tracing::span::Id, _values: &tracing::span::Record<'_>) {}

    fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {}

    fn event(&self, event: &tracing::Event<'_>) {
        let mut message = Message::default();
        event.record(&mut message);

        let level = match *event.metadata().level() {
            tracing::Level::ERROR => Level::Error,
            tracing::Level::WARN => Level::Warn,
            tracing::Level::INFO => Level::Info,
            tracing::Level::DEBUG => Level::Debug,
            tracing::Level::TRACE => Level::Trace,
        };

        log::log!(level, "hudhook: {}", message.0);
    }

    fn enter(&self, _span: &tracing::span::Id) {}

    fn exit(&self, _span: &tracing::span::Id) {}
}

// --------------------------------------------------------------------- paths

/// CLEO's log directory: the game directory unless `[General] LogDirectory` moved it.
fn log_dir() -> PathBuf {
    LOG_DIR
        .get_or_init(|| {
            cleo_string(unsafe { CLEO_GetLogDirectory() })
                .map(PathBuf::from)
                .or_else(|| exe_path().parent().map(Path::to_path_buf))
                .unwrap_or_else(|| PathBuf::from("."))
        })
        .clone()
}

fn exe_path() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."))
}

fn cleo_string(pointer: *const i8) -> Option<String> {
    if pointer.is_null() {
        return None;
    }

    let text = unsafe { CStr::from_ptr(pointer) }
        .to_str()
        .ok()?
        .to_string();
    (!text.is_empty()).then_some(text)
}

fn cs(value: &str) -> CString {
    CString::new(value.replace('\0', "")).unwrap_or_default()
}

/// `2026-11-24 14:22:05.443 ` - local time, because the file is read next to a wall clock.
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

#[cfg(test)]
mod tests {
    use super::{mirrored, stamp, Level};

    #[test]
    fn cleo_log_gets_milestones_and_warnings_but_not_per_event_detail() {
        assert!(mirrored(Level::Error));
        assert!(mirrored(Level::Warn));
        assert!(mirrored(Level::Info));
        assert!(!mirrored(Level::Debug));
        assert!(!mirrored(Level::Trace));
    }

    #[test]
    fn the_stamp_has_the_expected_shape() {
        let stamp = stamp();
        // "YYYY-MM-DD HH:MM:SS.mmm "
        assert_eq!(stamp.len(), 24, "{stamp:?}");
        assert_eq!(&stamp[4..5], "-");
        assert_eq!(&stamp[10..11], " ");
        assert_eq!(&stamp[19..20], ".");
        assert!(stamp.ends_with(' '));
    }
}
