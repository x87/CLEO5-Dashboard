//! Optional CLEO plugins, resolved at runtime.
//!
//! `SA.Audio.cleo` and `SA.MemoryOperations.cleo` are not linked statically: when they are
//! missing, the overlay still works and the matching tab is not shown. They are looked up
//! once, [`DETECT_DELAY_MS`] after the overlay starts, so CLEO has finished loading them.

use std::ffi::CString;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use hudhook::windows::core::PCSTR;
use hudhook::windows::Win32::Foundation::HMODULE;
use hudhook::windows::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};

const AUDIO_MODULE: &str = "SA.Audio.cleo";
const MEMORY_MODULE: &str = "SA.MemoryOperations.cleo";

/// Grace period after the overlay starts, before the one lookup.
const DETECT_DELAY_MS: u64 = 2000;

#[derive(Clone, Copy)]
pub struct AudioApi {
    pub number_of_streams: unsafe extern "system" fn() -> usize,
    pub filename_by_index: unsafe extern "system" fn(usize, *mut i8, usize) -> bool,
    pub length_by_index: unsafe extern "system" fn(usize) -> f32,
    pub state_by_index: unsafe extern "system" fn(usize) -> i32,
    pub type_by_index: unsafe extern "system" fn(usize) -> i32,
    pub volume_by_index: unsafe extern "system" fn(usize) -> f32,
    pub speed_by_index: unsafe extern "system" fn(usize) -> f32,
    pub is_3d_by_index: unsafe extern "system" fn(usize) -> bool,
    pub is_looped_by_index: unsafe extern "system" fn(usize) -> bool,
}

#[derive(Clone, Copy)]
pub struct MemoryApi {
    pub number_of_libraries: unsafe extern "system" fn() -> usize,
    pub library_by_index: unsafe extern "system" fn(usize, &mut HMODULE, &mut usize) -> bool,
    pub allocation_limits: unsafe extern "system" fn(&mut usize, &mut usize),
    pub allocation_info:
        unsafe extern "system" fn(*const crate::sdk::CRunningScript, &mut usize, &mut usize),
}

static AUDIO: Mutex<Option<AudioApi>> = Mutex::new(None);
static MEMORY: Mutex<Option<MemoryApi>> = Mutex::new(None);
static DONE: AtomicBool = AtomicBool::new(false);
static STARTED: OnceLock<Instant> = OnceLock::new();

/// `Some` when `SA.Audio.cleo` is loaded and every symbol resolved.
pub fn audio() -> Option<AudioApi> {
    *AUDIO.lock().unwrap()
}

/// `Some` when `SA.MemoryOperations.cleo` is loaded and every symbol resolved.
pub fn memory() -> Option<MemoryApi> {
    *MEMORY.lock().unwrap()
}

/// Resolve the plugins, once. Called every frame; the lookup itself runs
/// [`DETECT_DELAY_MS`] after the first call and never again.
pub fn detect() {
    if DONE.load(Ordering::Relaxed) {
        return;
    }

    let started = STARTED.get_or_init(Instant::now);
    if started.elapsed() < Duration::from_millis(DETECT_DELAY_MS) {
        return;
    }

    DONE.store(true, Ordering::Relaxed);

    if let Some(api) = unsafe { load_audio() } {
        log::info!("{AUDIO_MODULE} resolved, audio tab enabled");
        *AUDIO.lock().unwrap() = Some(api);
    }

    if let Some(api) = unsafe { load_memory() } {
        log::info!("{MEMORY_MODULE} resolved, libraries tab enabled");
        *MEMORY.lock().unwrap() = Some(api);
    }
}

unsafe fn load_audio() -> Option<AudioApi> {
    let module = module_handle(AUDIO_MODULE)?;

    Some(AudioApi {
        number_of_streams: symbol(module, "GetNumberOfLoadedStreams", 0)?,
        filename_by_index: symbol(module, "GetStreamFilenameByIndex", 12)?,
        length_by_index: symbol(module, "GetStreamLengthByIndex", 4)?,
        state_by_index: symbol(module, "GetStreamStateByIndex", 4)?,
        type_by_index: symbol(module, "GetStreamTypeByIndex", 4)?,
        volume_by_index: symbol(module, "GetStreamVolumeByIndex", 4)?,
        speed_by_index: symbol(module, "GetStreamSpeedByIndex", 4)?,
        is_3d_by_index: symbol(module, "IsStream3dByIndex", 4)?,
        is_looped_by_index: symbol(module, "IsStreamLoopedByIndex", 4)?,
    })
}

unsafe fn load_memory() -> Option<MemoryApi> {
    let module = module_handle(MEMORY_MODULE)?;

    Some(MemoryApi {
        number_of_libraries: symbol(module, "GetNumberOfLoadedLibraries", 0)?,
        library_by_index: symbol(module, "GetLoadedLibraryByIndex", 12)?,
        allocation_limits: symbol(module, "GetMemoryAllocationLimits", 8)?,
        allocation_info: symbol(module, "GetScriptAllocationInfo", 12)?,
    })
}

/// Only detects already loaded modules; the overlay never forces a plugin in.
unsafe fn module_handle(name: &str) -> Option<HMODULE> {
    let name = CString::new(name).ok()?;
    GetModuleHandleA(PCSTR(name.as_ptr() as *const u8)).ok()
}

/// Look up an `extern "system"` export. The stdcall-decorated names are tried first, then
/// the plain one (cdecl builds, or plugins that drop the decoration).
unsafe fn symbol<T: Copy>(module: HMODULE, name: &str, argument_bytes: usize) -> Option<T> {
    let candidates = [
        format!("_{name}@{argument_bytes}"),
        format!("{}@{argument_bytes}", name),
        name.to_string(),
    ];

    for candidate in candidates {
        let Ok(candidate) = CString::new(candidate) else {
            continue;
        };

        if let Some(address) =
            GetProcAddress(module, PCSTR(candidate.as_ptr() as *const u8))
        {
            return Some(std::mem::transmute_copy(&address));
        }
    }

    log::warn!("missing export {name} in the plugin");
    None
}
