//! The engine's script threads: CLEO's scripts and the game's own, in one list.
//!
//! CLEO enumerates its own scripts through the SDK; the game's threads are only reachable
//! through `CTheScripts::pActiveScripts`, the linked list the engine walks every frame. CLEO
//! puts its scripts in that same list, so one walk sees everything and `bIsCustom` says which
//! half a thread belongs to.
//!
//! The two halves differ in identity and in what can be measured: a CLEO script is a file
//! and carries its size, while a game thread is identified by the file its code was loaded
//! from plus the name `03A4` gave it, and has no recorded length. Everything else - locals,
//! IP, wake time, the flags - is the same struct.

use std::ffi::CStr;

use crate::settings;
use crate::sdk::*;
use crate::utils::make_path_relative_to_exe;

/// Which half of the list a thread comes from.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A CLEO script, loaded from a `.cs` file.
    Cleo,
    /// A thread the game started itself, from `main.scm` or `script.img`.
    Game,
}

/// One thread, resolved once so a table row, a debug window and a callback all read the
/// same values.
#[derive(Clone)]
pub struct Entry {
    pub ptr: *mut CRunningScript,
    pub kind: Kind,
    /// The identity that survives the pointer changing: the absolute path of a CLEO
    /// script, `main.scm:NAME` for one of the game's.
    pub key: String,
    /// What the Scripts table shows in its script column: the path of a CLEO script, or
    /// `main.scm\` / `script.img\` followed by the name of one of the game's threads.
    pub label: String,
    /// The file name CLEO reports, or the `03A4` name of a game thread.
    pub name: String,
    /// `BaseIP`: the first byte of the thread's code.
    pub base: usize,
}

impl Entry {
    /// Whether the row describes one of CLEO's scripts rather than the game's.
    pub fn is_cleo(&self) -> bool {
        self.kind == Kind::Cleo
    }

    /// The row's name in the two pieces the table colours differently: the folder it came
    /// from and the name itself. A path with no folder has an empty first half.
    pub fn label_parts(&self) -> (String, String) {
        match self.label.rfind(['\\', '/']) {
            Some(index) => (
                self.label[..=index].to_string(),
                self.label[index + 1..].to_string(),
            ),
            None => (String::new(), self.label.clone()),
        }
    }

    /// The file the thread's code came out of, on its own: the `.cs` file of a CLEO script,
    /// or `main.scm` / `script.img` for one of the game's.
    pub fn source(&self) -> &str {
        match self.kind {
            Kind::Cleo => &self.name,
            Kind::Game => game_source(self.base),
        }
    }

    /// Whether either of the thread's names contains `needle`, which the caller lowercased.
    pub fn matches(&self, needle: &str) -> bool {
        needle.is_empty()
            || self.label.to_lowercase().contains(needle)
            || self.name.to_lowercase().contains(needle)
    }
}

/// A walk of a corrupt list must not hang the frame.
const MAX_QUEUE: usize = 4096;

/// `CRunningScript`'s name field, which the constructor fills with this.
const NO_NAME: &str = "noname";

/// Every thread in the engine's active queue, in the order the engine runs them.
fn queue() -> Vec<*mut CRunningScript> {
    let mut threads = Vec::new();
    let mut script = unsafe { *(ACTIVE_SCRIPTS as *const *mut CRunningScript) };

    while !script.is_null() && threads.len() < MAX_QUEUE {
        threads.push(script);
        script = unsafe { (*script).m_pNext as *mut CRunningScript };
    }

    threads
}

/// The two buffers the engine loads bytecode into, with their lengths. The mission block is
/// as big as the largest mission, which only exists at runtime; the retail size stands in
/// until the game has read the SCM header.
fn blocks() -> [(usize, usize); 2] {
    let scm = unsafe { CLEO_GetScmMainData() as usize };
    let mission = unsafe { *(MISSION_BLOCK_REF as *const usize) };
    let mission_size = match unsafe { *(LARGEST_MISSION_SIZE as *const usize) } {
        0 => MISSION_BLOCK_SIZE,
        size => size,
    };

    [(scm, SCM_BLOCK_SIZE), (mission, mission_size)]
}

/// The mission offsets the SCM header describes, and the largest mission. `0` until the game
/// has read `main.scm`.
fn missions() -> (Vec<u32>, usize) {
    let count = unsafe { *(MISSION_COUNT as *const usize) };
    let largest = unsafe { *(LARGEST_MISSION_SIZE as *const usize) };
    if count == 0 || count > 4096 {
        return (Vec::new(), largest);
    }

    let table = unsafe { std::slice::from_raw_parts(MISSION_OFFSETS as *const u32, count) };
    (table.to_vec(), largest)
}

/// How big the mission a thread is running through is, when the thread is one of the ones
/// loaded from `script.img`.
///
/// CLEO reports a mission's code position in `.SCM` coordinates (`ip + offset - scmBlock`),
/// so turning that around gives the mission's offset without asking which mission is loaded.
/// The next offset in the header's table bounds it; the last mission is bounded by the
/// largest one. An offset not in the table means nothing is claimed.
fn mission_size(script: *mut CRunningScript) -> Option<usize> {
    let (offsets, largest) = missions();
    if offsets.is_empty() || largest == 0 {
        return None;
    }

    // Only missions are loaded into the mission block; the threads `main.scm` starts run out
    // of the script block and have no entry in this table.
    let ip = unsafe { (*script).m_nIp };
    let mission = unsafe { *(MISSION_BLOCK_REF as *const usize) };
    let (_, block_size) = block_of(mission)?;
    if ip < mission || ip >= mission + block_size {
        return None;
    }

    let reported = unsafe { CLEO_GetScriptBaseRelativeOffset(script, ip as *const u8) } as usize;
    let scm = unsafe { CLEO_GetScmMainData() as usize };
    let offset = (reported + scm).checked_sub(ip)? as u32;
    let index = offsets.iter().position(|entry| *entry == offset)?;

    let end = offsets[index + 1..]
        .iter()
        .copied()
        .filter(|entry| *entry > offset)
        .min()
        .map(|entry| entry as usize)
        .unwrap_or(offset as usize + largest);

    Some(end.saturating_sub(offset as usize))
}

/// The block a code address belongs to.
fn block_of(address: usize) -> Option<(usize, usize)> {
    if address == 0 {
        return None;
    }

    blocks()
        .into_iter()
        .find(|(start, size)| address >= *start && address < start + size)
}

/// Whether the code at `base` was loaded from `script.img` rather than `main.scm`.
fn from_image(base: usize) -> bool {
    let mission = unsafe { *(MISSION_BLOCK_REF as *const usize) };
    base >= mission && base < mission + MISSION_BLOCK_SIZE
}

/// The absolute path of a CLEO script, which is what survives a reload: the pointer does
/// not.
fn full_path(script: *mut CRunningScript) -> String {
    let mut path = [0i8; 260];
    unsafe { CLEO_GetScriptFileFullPath(script, path.as_mut_ptr(), path.len() as u32) };

    if path[0] == 0 {
        return String::new();
    }

    unsafe { CStr::from_ptr(path.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

/// The file a game thread came out of, as the row names it: one of the two buffers the
/// engine loads bytecode into.
fn game_source(base: usize) -> &'static str {
    if from_image(base) {
        "script.img"
    } else {
        "main.scm"
    }
}

/// The name a game thread goes by: the one `03A4` gave it, or - for a thread that never
/// named itself - the offset it starts at.
fn game_name(script: *mut CRunningScript) -> String {
    let name = unsafe { (*script).name() };
    if !name.is_empty() && name != NO_NAME {
        return name;
    }

    let base = unsafe { (*script).m_nBaseIp };
    let offset = block_of(base)
        .map(|(start, _)| base - start)
        .unwrap_or(base);
    format!("@{offset:05X}")
}

/// The identity of one of the game's threads: the file it came from and the name above.
fn game_key(script: *mut CRunningScript) -> String {
    let base = unsafe { (*script).m_nBaseIp };
    format!("{}:{}", game_source(base), game_name(script))
}

/// The queue, minus the threads CLEO already covers.
fn game_threads() -> impl Iterator<Item = *mut CRunningScript> {
    queue()
        .into_iter()
        .filter(|script| !unsafe { (**script).is_custom() })
}

fn cleo_entry(index: usize) -> Entry {
    let script = unsafe { CLEO_GetCustomScriptByIndex(index) };
    let key = full_path(script);
    let name = unsafe { CLEO_GetScriptFilename(script) };

    Entry {
        ptr: script,
        kind: Kind::Cleo,
        label: make_path_relative_to_exe(&key),
        name: if name.is_null() {
            String::new()
        } else {
            unsafe { CStr::from_ptr(name) }.to_string_lossy().into_owned()
        },
        key,
        base: unsafe { (*script).m_nBaseIp },
    }
}

fn game_entry(script: *mut CRunningScript) -> Entry {
    let base = unsafe { (*script).m_nBaseIp };
    let source = game_source(base);
    let name = game_name(script);

    Entry {
        ptr: script,
        kind: Kind::Game,
        key: format!("{source}:{name}"),
        // The same shape as a CLEO row: the file it came from, then the name that follows
        // it - `main.scm\`, `script.img\`.
        label: format!("{source}\\{name}"),
        name,
        base,
    }
}

/// Every thread: CLEO's scripts in CLEO's own order, then the game's own in queue order.
pub fn all() -> Vec<Entry> {
    let mut entries: Vec<Entry> = (0..unsafe { CLEO_GetNumberOfCustomScripts() })
        .map(cleo_entry)
        .collect();

    entries.extend(game_threads().map(game_entry));
    entries
}

/// How many threads a table will list.
pub fn count() -> usize {
    let cleo = unsafe { CLEO_GetNumberOfCustomScripts() };
    cleo + game_threads().count()
}

/// The thread `key` names, resolved fresh this frame.
pub fn find(key: &str) -> Option<Entry> {
    for index in 0..unsafe { CLEO_GetNumberOfCustomScripts() } {
        let script = unsafe { CLEO_GetCustomScriptByIndex(index) };
        if full_path(script) == key {
            return Some(cleo_entry(index));
        }
    }

    game_threads()
        .find(|script| game_key(*script) == key)
        .map(game_entry)
}

/// The thread at `pointer`, if the queue still holds it.
pub fn at(pointer: usize) -> Option<Entry> {
    if pointer == 0 {
        return None;
    }

    for index in 0..unsafe { CLEO_GetNumberOfCustomScripts() } {
        if unsafe { CLEO_GetCustomScriptByIndex(index) } as usize == pointer {
            return Some(cleo_entry(index));
        }
    }

    game_threads()
        .find(|script| *script as usize == pointer)
        .map(game_entry)
}

/// The `BaseIP` of every thread that records one, so a caller measuring a whole table pays
/// for one walk instead of one per row.
pub fn bases() -> Vec<usize> {
    queue()
        .into_iter()
        .filter(|script| has_start(unsafe { &**script }))
        .map(|script| unsafe { (*script).m_nBaseIp })
        .collect()
}

/// Where a thread's code is read from: its `BaseIP`, or - when the engine left that zero,
/// which it does for the threads `main.scm` starts - the script block, the only thing every
/// offset can be counted from.
pub fn code_base(script: *mut CRunningScript) -> usize {
    if script.is_null() {
        return 0;
    }

    let base = unsafe { (*script).m_nBaseIp };
    if base != 0 {
        return base;
    }

    unsafe { CLEO_GetScmMainData() as usize }
}

/// Whether the engine recorded where the thread's script begins. It has for a CLEO script
/// and for a mission loaded out of `script.img`; the threads `main.scm` starts carry only a
/// current position.
pub fn has_start(script: *const CRunningScript) -> bool {
    unsafe { (*script).m_nBaseIp != 0 }
}

/// Whether CLEO still owns this script pointer. A script that ended, or is waiting to be
/// deleted, leaves its address behind, so every write through a script pointer asks this
/// first; a read that only shows a number asks it too.
pub fn alive(script: *mut CRunningScript) -> bool {
    !script.is_null() && unsafe { CLEO_IsValidScriptPtr(script) } != 0
}

/// `m_nWakeTime` is the only thing that says whether a thread is parked: the engine skips it
/// while the value is greater than the game clock, and `0xFFFFFFFF` parks it for good.
pub fn is_suspended(script: *mut CRunningScript) -> bool {
    alive(script) && unsafe { (*script).m_nWakeTime } == SUSPENDED_WAKE_TIME
}

/// Park a running thread, or let a parked one run again.
///
/// A mission thread of the game stays where it is unless the setting that allows parking one
/// is on, and a pointer the game has already freed is left alone.
pub fn toggle_suspended(script: *mut CRunningScript) {
    if !alive(script) {
        return;
    }
    if !unsafe { (*script).is_custom() }
        && unsafe { (*script).m_bIsMission }
        && !settings::allow_native_control()
    {
        return;
    }

    let wake_time = if is_suspended(script) {
        0
    } else {
        SUSPENDED_WAKE_TIME
    };
    unsafe { (*script).m_nWakeTime = wake_time };
}

/// How many bytes may be read forward from [`code_base`]: a CLEO script's own length, the
/// region a game thread occupies, or - for a thread whose start was never recorded - the
/// rest of its block.
pub fn readable(script: *mut CRunningScript) -> Option<usize> {
    readable_between(script, &bases())
}

/// [`readable`], with the queue's base addresses already collected.
pub fn readable_between(script: *mut CRunningScript, bases: &[usize]) -> Option<usize> {
    if script.is_null() {
        return None;
    }

    if unsafe { (*script).is_custom() } {
        let size = unsafe { CLEO_GetScriptSize(script) };
        return (size != 0).then_some(size);
    }

    let base = code_base(script);
    let (start, size) = block_of(base)?;
    let end = start + size;

    if has_start(script) {
        Some(region_end(base, end, bases) - base)
    } else if let Some(size) = mission_size(script) {
        // A mission knows its own extent from the header, so reads stop at its end rather
        // than at the end of the block it was loaded into.
        Some(unsafe { (*script).m_nIp }.saturating_sub(base) + size)
    } else {
        // Nothing says where the script begins, so everything from the block's start to its
        // end is fair game; the listing decides where to open.
        Some(end - base)
    }
}

/// How big a game thread's script is, as far as anything can tell: measured to the next
/// script in its block, taken from the mission table, or unknown when the thread records no
/// start at all.
pub fn region_between(script: *mut CRunningScript, bases: &[usize]) -> Option<usize> {
    if script.is_null() {
        return None;
    }

    if unsafe { (*script).is_custom() } {
        let size = unsafe { CLEO_GetScriptSize(script) };
        return (size != 0).then_some(size);
    }

    if has_start(script) {
        let base = unsafe { (*script).m_nBaseIp };
        let (start, size) = block_of(base)?;
        return Some(region_end(base, start + size, bases) - base);
    }

    mission_size(script)
}

/// Where the region of the script starting at `base` stops: at the next script's `BaseIP` in
/// the same block, or at the block's own end.
///
/// The queue is in the order the engine runs threads, not the order their code sits in
/// memory, so the next script is the smallest `BaseIP` above this one. Equal addresses are
/// skipped: two threads started from the same label share a `BaseIP`.
fn region_end(base: usize, block_end: usize, bases: &[usize]) -> usize {
    bases
        .iter()
        .copied()
        .filter(|other| *other > base && *other < block_end)
        .min()
        .unwrap_or(block_end)
}

/// Whether a thread's `03A4` name is `name`: the whole name, then nothing but the zeroes
/// the field was padded with.
pub fn name_matches(raw: &[u8; 8], name: &str) -> bool {
    let bytes = name.as_bytes();

    bytes.len() <= raw.len()
        && raw[..bytes.len()] == *bytes
        && raw[bytes.len()..].iter().all(|byte| *byte == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(label: &str, name: &str) -> Entry {
        Entry {
            ptr: std::ptr::null_mut(),
            kind: Kind::Cleo,
            key: format!("{label}:{name}"),
            label: label.to_string(),
            name: name.to_string(),
            base: 0,
        }
    }

    #[test]
    fn the_filter_box_matches_either_name_in_any_case() {
        let thread = entry("cleo\\SPRLA.cs", "sprla");

        assert!(thread.matches(""));
        assert!(thread.matches("sprla"));
        assert!(thread.matches("cleo"));
        assert!(!thread.matches("psch"));
    }

    #[test]
    fn a_region_ends_where_the_next_script_starts() {
        let bases = [0x1_0000, 0x1_0100, 0x0_9000];
        assert_eq!(region_end(0x1_0000, 0x2_0000, &bases), 0x1_0100);
    }

    #[test]
    fn a_thread_without_a_recorded_start_has_no_region() {
        assert_eq!(region_end(0x1_0000, 0x1_0000, &[]), 0x1_0000);
    }

    #[test]
    fn the_last_script_in_a_block_ends_with_the_block() {
        let bases = [0x1_0000, 0x1_0100];
        assert_eq!(region_end(0x1_0100, 0x2_0000, &bases), 0x2_0000);
    }

    #[test]
    fn a_thread_sharing_a_label_does_not_end_the_other_ones_region() {
        let bases = [0x1_0000, 0x1_0000, 0x1_0200];
        assert_eq!(region_end(0x1_0000, 0x2_0000, &bases), 0x1_0200);
    }

    #[test]
    fn a_script_in_another_block_is_not_the_next_one() {
        let bases = [0x1_0000, 0x3_0000];
        assert_eq!(region_end(0x1_0000, 0x2_0000, &bases), 0x2_0000);
    }

    #[test]
    fn a_name_matches_up_to_its_terminator_and_no_further() {
        let mut raw = [0u8; 8];
        raw[..5].copy_from_slice(b"SPRLA");

        assert!(name_matches(&raw, "SPRLA"));
        assert!(!name_matches(&raw, "SPRL"));
        assert!(!name_matches(&raw, "SPRLAB"));

        // An eight-character name fills the field and has no terminator of its own.
        let mut full = [0u8; 8];
        full.copy_from_slice(b"MISSIONX");
        assert!(name_matches(&full, "MISSIONX"));
    }
}
