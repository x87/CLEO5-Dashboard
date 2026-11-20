//! The modules the game process has loaded, which is what the Libraries tab lists.
//!
//! `SA.MemoryOperations.cleo` knows only the libraries a script asked for - a handful of
//! rows next to the sixty-odd modules a modded game has in memory - and the question the
//! tab gets asked ("is that ASI actually loaded?") is about the ones CLEO never touched.
//! So the list comes from the process itself, and the plugin only adds the reference counts
//! for the modules it does know.

use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use hudhook::windows::Win32::Foundation::CloseHandle;
use hudhook::windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Module32First, Module32Next, MODULEENTRY32, TH32CS_SNAPMODULE,
    TH32CS_SNAPMODULE32,
};
use hudhook::windows::Win32::System::SystemInformation::GetWindowsDirectoryA;
use hudhook::windows::Win32::System::Threading::GetCurrentProcessId;

use crate::utils::make_path_relative_to_exe;

/// What a module's file name says it is. One kind per extension, and the tab's chips are
/// exactly these: the extension decides what a module is, not who loaded it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The game's own executable.
    Exe,
    /// An ASI mod, loaded by an ASI loader.
    Asi,
    /// A CLEO plugin, loaded by CLEO.
    Cleo,
    /// A plain DLL: the game's, the system's, or a mod that ships one.
    Dll,
    /// A file name with none of those extensions.
    Other,
}

impl Kind {
    fn of(name: &str) -> Kind {
        let extension = name
            .rsplit_once('.')
            .map(|(_, extension)| extension.to_ascii_lowercase());
        match extension.as_deref() {
            Some("exe") => Kind::Exe,
            Some("asi") => Kind::Asi,
            Some("cleo") => Kind::Cleo,
            Some("dll") => Kind::Dll,
            _ => Kind::Other,
        }
    }
}

pub struct Module {
    /// Path relative to the game directory, as the table shows it.
    pub path: String,
    /// The same path lower-cased, for the filter box and the sort: both run over the whole
    /// list, and lower-casing eighty paths on every frame is work for nothing.
    pub path_lower: String,
    /// Loaded image base. This is the `HMODULE` the module has, which is also how
    /// `SA.MemoryOperations.cleo` reports the libraries it knows about.
    pub base: usize,
    pub kind: Kind,
    /// Whether the image sits in the Windows directory (`System32`, `WinSxS`, the
    /// `DriverStore`). Those are the rows the tab hides by default: they are the system's,
    /// not the modder's.
    pub system: bool,
}

/// How long a snapshot is reused: modules only appear when a script loads another library.
const CACHE_MS: u64 = 1000;

/// The snapshot and when it was taken.
type Cache = Mutex<(Option<Instant>, Arc<Vec<Module>>)>;

static CACHE: OnceLock<Cache> = OnceLock::new();

/// Every module loaded in this process. At most [`CACHE_MS`] old.
pub fn loaded() -> Arc<Vec<Module>> {
    let mut cache = CACHE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let fresh = cache
        .0
        .is_some_and(|at| at.elapsed() < Duration::from_millis(CACHE_MS));
    if !fresh {
        // A failed walk leaves the list empty, which is never right: keep the last good
        // snapshot and try again next second.
        let modules = enumerate();
        if !modules.is_empty() {
            cache.1 = Arc::new(modules);
        }
        cache.0 = Some(Instant::now());
    }

    Arc::clone(&cache.1)
}

/// Whether a module with this file name is loaded, case-insensitively. Matched on the last
/// path component: the plugin may sit in a subdirectory of the game directory.
pub fn is_loaded(file_name: &str) -> bool {
    let needle = file_name.to_ascii_lowercase();
    loaded().iter().any(|module| {
        module.path_lower == needle
            || module
                .path_lower
                .rsplit_once('\\')
                .is_some_and(|(_, name)| name == needle)
    })
}

/// The Windows directory, lower-cased and with its trailing separator, read once.
fn windows_dir() -> Option<&'static str> {
    static DIR: OnceLock<Option<String>> = OnceLock::new();

    DIR.get_or_init(|| {
        let mut buffer = [0u8; 260];
        let len = unsafe { GetWindowsDirectoryA(Some(&mut buffer)) } as usize;
        if len == 0 || len >= buffer.len() {
            return None;
        }

        let dir = std::str::from_utf8(&buffer[..len]).ok()?;
        Some(format!("{}\\", dir.to_ascii_lowercase()))
    })
    .as_deref()
}

/// Whether a module path sits in the Windows directory (`System32`, `SysWOW64`, `WinSxS`,
/// `DriverStore`). The trailing separator matters, or `C:\WindowsApps` would count.
fn in_windows_dir(absolute: &str) -> bool {
    windows_dir().is_some_and(|dir| absolute.to_ascii_lowercase().starts_with(dir))
}

fn enumerate() -> Vec<Module> {
    // The snapshot is a copy, so the walk cannot see the loader list change under it. Taking
    // the copy can still lose the race with a module loading elsewhere - `ERROR_BAD_LENGTH`,
    // which Win32 asks to be retried.
    let mut snapshot = None;
    for _ in 0..4 {
        match unsafe {
            CreateToolhelp32Snapshot(
                TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32,
                GetCurrentProcessId(),
            )
        } {
            Ok(handle) => {
                snapshot = Some(handle);
                break;
            }
            Err(_) => continue,
        }
    }

    let Some(snapshot) = snapshot else {
        log::warn!("could not snapshot the loaded modules");
        return Vec::new();
    };

    let mut modules = Vec::new();
    let mut entry = MODULEENTRY32 {
        dwSize: std::mem::size_of::<MODULEENTRY32>() as u32,
        ..Default::default()
    };

    // Both calls fill the same entry, and `dwSize` is set only once.
    let mut more = unsafe { Module32First(snapshot, &mut entry) }.is_ok();
    while more {
        let absolute = unsafe { std::ffi::CStr::from_ptr(entry.szExePath.as_ptr()) };
        let name = unsafe { std::ffi::CStr::from_ptr(entry.szModule.as_ptr()) };
        if let (Ok(absolute), Ok(name)) = (absolute.to_str(), name.to_str()) {
            let path = make_path_relative_to_exe(absolute);
            modules.push(Module {
                path_lower: path.to_ascii_lowercase(),
                path,
                base: entry.modBaseAddr as usize,
                kind: Kind::of(name),
                system: in_windows_dir(absolute),
            });
        }

        more = unsafe { Module32Next(snapshot, &mut entry) }.is_ok();
    }

    let _ = unsafe { CloseHandle(snapshot) };

    // Load order interleaves the game with system DLLs and moves whenever a script loads
    // something; the executable first and the rest by name keeps rows where the eye left them.
    modules.sort_by_cached_key(|module| (module.kind != Kind::Exe, module.path_lower.clone()));

    modules
}

#[cfg(test)]
mod tests {
    use super::Kind;

    #[test]
    fn kind_comes_from_the_extension_alone() {
        assert_eq!(Kind::of("gta_sa.exe"), Kind::Exe);
        assert_eq!(Kind::of("GTA_SA.EXE"), Kind::Exe);
        assert_eq!(Kind::of("SA.Dashboard.cleo"), Kind::Cleo);
        assert_eq!(Kind::of("modloader.asi"), Kind::Asi);
        assert_eq!(Kind::of("vorbisFile.dll"), Kind::Dll);
        assert_eq!(Kind::of("cleo\\lib\\libstdc++-6.dll"), Kind::Dll);
    }

    #[test]
    fn a_name_without_a_known_extension_is_other() {
        assert_eq!(Kind::of("no-extension"), Kind::Other);
        assert_eq!(Kind::of("archive.asi.bak"), Kind::Other);
    }
}
