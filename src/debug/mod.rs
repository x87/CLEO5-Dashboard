//! Per-script debug view: what a script is doing, what its variables hold, and how to stop it
//! between two instructions.
//!
//! Built on the shipping CLEO SDK only: two opcode callbacks (trace and control), one log
//! callback for the reason CLEO suspended a script, and `m_nWakeTime` writes for parking.
//!
//! The callbacks run on the game thread once per instruction for every script, so they leave
//! as fast as they entered: one relaxed atomic load while nothing is traced.

mod db;
mod decode;
mod enums;
mod listing;
mod render;
mod trace;
mod window;

/// Line cap bounds the panel offers, re-exported for [`crate::settings`].
pub const DEFAULT_LINE_CAP: usize = trace::DEFAULT_CAPACITY;

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use imgui::Ui;

use crate::debug::db::Db;
use crate::debug::decode::{Plan, Scope, VarKind};
use crate::debug::render::{LabelStyle, Layout, VarSource};
use crate::debug::trace::Ring;
use crate::nav::Intent;
use crate::sdk::*;
use crate::settings;
use crate::threads::{self, Kind};

/// `if`, which is also the and/or marker: a line that follows one is part of a condition
/// list and carries one more level of indentation.
const COMMAND_IF: u16 = 0x00D6;

/// CLEO's global variable space: `SCRIPT_VAR[0x4000]` at the base of the main script
/// block. Nothing above that is a global variable, so nothing above it is read.
const MAX_GLOBALS: usize = 0x4000;

/// `wait 0`: run again as soon as the script is reached.
const WAKE_IMMEDIATELY: u32 = 0;

// --------------------------------------------------------------------- state

/// What the panel asked the after-callback to do, applied on the next instruction.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Request {
    Pause,
    Resume,
    /// Run this many instructions, then park again.
    Step(i32),
}

#[derive(Default)]
struct Control {
    request: Option<Request>,
    /// The parking is ours: the panel may resume the script.
    paused: bool,
    /// Instructions still to run before parking again.
    budget: i32,
    /// The wake time the script had before it was parked, so a resume keeps the rest of a
    /// `wait` instead of cutting it short.
    saved_wake: Option<u32>,
}

/// Which half the left pane shows.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pane {
    /// The script decoded end to end, with the line it is standing on marked.
    Disassembly,
    /// The instructions that have already run, newest last.
    Trace,
}

/// One script the panel knows about.
struct ScriptDebug {
    /// Identity: the absolute path of a CLEO script, the queue name of one of the game's.
    key: String,
    /// File name as CLEO reports it, or the `03A4` name of a game thread. Both are what the
    /// callback can compare against, but they come from different fields - see [`Kind`].
    name: String,
    /// Which half of the engine's thread list the script is from.
    kind: Kind,
    /// The live thread, refreshed every frame the window is drawn.
    ptr: usize,
    /// Bytes of bytecode from `BaseIP`, refreshed with the thread: exact for a CLEO script,
    /// the region it occupies for one of the game's.
    code_size: usize,
    /// Which half of the engine's thread list the script is from.
    trace: bool,
    control: Control,
    ring: Ring,
    /// Last opcode, for the condition-list indentation rule.
    prev_opcode: u16,
    /// What the trace has learned about the 32 locals, for the grid.
    local_types: LocalTypes,
    /// Which pane the left half shows.
    pane: Pane,
    /// The script decoded into instructions, built once per (thread, size).
    listing: Option<listing::Listing>,
    /// The `(ptr, size, first offset)` the listing was built for, so a reload - or a game
    /// thread moving past what was decoded - rebuilds it.
    listing_for: (usize, usize, u32),
    /// Keep the marked line (and the newest trace line) in view. Turned off by scrolling up
    /// and by walking the list with the keys; turned back on by the switch, by `Resume` and
    /// by `Step`.
    auto_scroll: bool,
    /// The trace line the keys are on, counted from the newest recorded line, so a line the
    /// script runs later - which arrives at the end - cannot move the cursor.
    trace_cursor: Option<usize>,
    /// The listing line the keys are on. The listing is built once per thread and never
    /// renumbers, so an absolute index is stable here where the trace's is not.
    disasm_cursor: Option<usize>,
    /// Split between the listing and the variables, as a fraction of the pane width. Live
    /// while the handle is dragged, written to the ini on release.
    split: f32,
    /// Height of what sits under the panes (separator and fields header), measured as it is
    /// drawn so the panes get exactly the rest. Zero until the first frame has drawn it.
    tail: f32,
    /// Ask for the window to be raised on its next draw, so opening one that is already up
    /// does not look like nothing happened.
    focus: bool,
}

impl ScriptDebug {
    fn new(key: String, name: String, kind: Kind, capacity: usize) -> Self {
        Self {
            key,
            name,
            kind,
            ptr: 0,
            code_size: 0,
            // Recording is not a switch: a script with a window open is always traced,
            // from the moment it is opened.
            trace: true,
            control: Control::default(),
            ring: Ring::new(capacity),
            prev_opcode: 0xFFFF,
            local_types: LocalTypes::default(),
            pane: Pane::Disassembly,
            listing: None,
            listing_for: (0, 0, 0),
            auto_scroll: true,
            trace_cursor: None,
            disasm_cursor: None,
            split: settings::debug_split(),
            tail: 0.0,
            focus: false,
        }
    }
}

struct State {
    scripts: Vec<ScriptDebug>,
    /// Absolute paths with an open window, in the order they were opened.
    windows: Vec<String>,
    /// The window that had the focus on its last draw, which is the one `Escape` closes.
    /// `None` while the sheet itself is the window the user is in.
    focused: Option<String>,
    /// Last message CLEO logged at error level, used to explain a suspension.
    last_error: Option<String>,
}

static STATE: OnceLock<Mutex<State>> = OnceLock::new();

/// Set while at least one script is traced or controlled, so the callbacks leave after one
/// atomic load in the common case.
static ACTIVE: AtomicBool = AtomicBool::new(false);

/// The one script the callbacks care about, so the common case - a single script being
/// debugged - costs one pointer compare instead of a mutex and a scan. Zero means "more
/// than one script is being watched, look under the lock".
static WATCH: AtomicUsize = AtomicUsize::new(0);

/// The command database, parsed on first use.
static DB: OnceLock<Option<Db>> = OnceLock::new();

fn db() -> Option<&'static Db> {
    DB.get_or_init(db::load).as_ref()
}

fn state() -> &'static Mutex<State> {
    STATE.get_or_init(|| {
        Mutex::new(State {
            scripts: Vec::new(),
            windows: Vec::new(),
            focused: None,
            last_error: None,
        })
    })
}

/// Recompute the two guards the callbacks read before they take the lock.
impl State {
}

fn refresh_active(state: &State) {
    let mut watched = state
        .scripts
        .iter()
        .filter(|script| script.trace || script.control.paused || script.control.request.is_some())
        .map(|script| script.ptr);

    let first = watched.next().unwrap_or(0);
    let single = watched.next().is_none();

    ACTIVE.store(first != 0, Ordering::Relaxed);
    WATCH.store(if first == 0 || single { first } else { usize::MAX }, Ordering::Relaxed);
}

/// `false` when this pointer is definitely not one the callbacks care about.
fn watching(script: *mut CRunningScript) -> bool {
    let watch = WATCH.load(Ordering::Relaxed);
    watch == 0 || watch == usize::MAX || watch == script as usize
}

// --------------------------------------------------------------------- hooks

/// Register the three callbacks. Called once, from the overlay's `initialize`.
pub fn install() {
    if !settings::debug_enabled() {
        log::info!("script debug: disabled by Dashboard.Debug.Enabled=0");
        return;
    }

    let before: OpcodeHookBefore = on_opcode_before;
    let after: OpcodeHookAfter = on_opcode_after;
    let log_hook: LogHook = on_log;

    unsafe {
        CLEO_RegisterCallback(CB_OPCODE_BEFORE, before as *mut c_void);
        CLEO_RegisterCallback(CB_OPCODE_AFTER, after as *mut c_void);
        CLEO_RegisterCallback(CB_LOG, log_hook as *mut c_void);
    }

    log::info!("script debug: opcode callbacks registered");
}

/// Apply the work a drawn window queued: windows never lock the state they draw from.
pub fn tick() {
    // The same for windows opened from inside a window, e.g. the Next button.
    let pending = PENDING_OPEN.lock().map(|mut queued| std::mem::take(&mut *queued));
    if let Ok(queued) = pending {
        for (key, name, kind) in queued {
            open(&key, &name, kind);
        }
    }
}

/// Windows a drawn window asked to open. Applied by [`tick`], outside the draw.
static PENDING_OPEN: Mutex<Vec<(String, String, Kind)>> = Mutex::new(Vec::new());

/// Open a script's debugger on the next [`tick`], from inside the panel's own draw.
pub(super) fn request_open(key: &str, name: &str, kind: Kind) {
    if let Ok(mut queued) = PENDING_OPEN.lock() {
        queued.push((key.to_string(), name.to_string(), kind));
    }
}

/// The before-opcode callback: decode, render, drop the line in the ring. Returning
/// `OR_NONE` leaves the instruction to CLEO's own handler; this only reads the byte stream.
extern "system" fn on_opcode_before(script: *mut CRunningScript, opcode: u32) -> i8 {
    if !ACTIVE.load(Ordering::Relaxed) || script.is_null() || !watching(script) {
        return OR_NONE;
    }

    // `try_lock`: the callback can be reached from inside a CLEO call the panel made while
    // it holds this mutex. A missed line beats a deadlock on the game thread.
    let Ok(mut state) = state().try_lock() else {
        return OR_NONE;
    };


    let Some(entry) = state
        .scripts
        .iter_mut()
        .find(|entry| is_script(entry, script))
    else {
        return OR_NONE;
    };
    if !entry.trace {
        return OR_NONE;
    }

    let opcode = opcode as u16;
    if let Some(line) = record(entry, script, opcode) {
        entry.ring.push(line);
    }
    entry.prev_opcode = opcode;

    OR_NONE
}

/// The after-opcode callback: applies pause/step and returns `OR_INTERRUPT` to stop the
/// script's loop right here.
extern "system" fn on_opcode_after(script: *mut CRunningScript, opcode: u32, result: i8) -> i8 {
    // Inside the engine's script loop, through CLEO's callback: a panic unwinding out of it
    // aborts the game, so it is caught here.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        after_opcode(script, opcode, result)
    }))
    .unwrap_or(result)
}

fn after_opcode(script: *mut CRunningScript, _opcode: u32, result: i8) -> i8 {
    if !ACTIVE.load(Ordering::Relaxed) || script.is_null() || !watching(script) {
        return result;
    }

    let Ok(mut state) = state().try_lock() else {
        return result;
    };

    let Some(entry) = state
        .scripts
        .iter_mut()
        .find(|entry| is_script(entry, script))
    else {
        return result;
    };

    let wake = unsafe { &mut (*script).m_nWakeTime };

    if let Some(request) = entry.control.request.take() {
        return match request {
            Request::Pause => {
                entry.control.paused = true;
                entry.control.budget = 0;
                entry.control.remember_wake(*wake);
                *wake = SUSPENDED_WAKE_TIME;
                OR_INTERRUPT
            }
            Request::Resume => {
                entry.control.paused = false;
                entry.control.budget = 0;
                *wake = entry.control.saved_wake.take().unwrap_or(WAKE_IMMEDIATELY);
                OR_NONE
            }
            Request::Step(count) => {
                entry.control.paused = true;
                entry.control.budget = (count - 1).max(0);
                // Stepping means "run now": the remaining wait is deliberately dropped.
                entry.control.saved_wake = None;
                if entry.control.budget > 0 {
                    *wake = WAKE_IMMEDIATELY;
                    OR_NONE
                } else {
                    *wake = SUSPENDED_WAKE_TIME;
                    OR_INTERRUPT
                }
            }
        };
    }

    if !entry.control.paused {
        return result;
    }

    // Paused between instructions: run the rest of the step budget, then park again.
    if entry.control.budget > 0 {
        entry.control.budget -= 1;
        if entry.control.budget > 0 {
            *wake = WAKE_IMMEDIATELY;
            return OR_NONE;
        }
    }

    *wake = SUSPENDED_WAKE_TIME;
    OR_INTERRUPT
}

impl Control {
    /// Keep the wake time a script had, unless it was already parked by somebody else.
    fn remember_wake(&mut self, wake: u32) {
        self.saved_wake = if wake == SUSPENDED_WAKE_TIME {
            None
        } else {
            Some(wake)
        };
    }
}

/// The log callback, used for one thing: remembering why CLEO suspended a script. The SDK
/// reports the wake-time marker but not the message that set it, and the message is what
/// the panel shows.
extern "system" fn on_log(level: u32, message: *const i8) {
    // eLogLevel::Error = 2; everything else is progress noise.
    if level != 2 || message.is_null() {
        return;
    }

    let Ok(text) = unsafe { std::ffi::CStr::from_ptr(message) }.to_str() else {
        return;
    };

    // `try_lock`: CLEO logs from inside calls the panel makes while it holds this mutex.
    if let Ok(mut state) = state().try_lock() {
        state.last_error = Some(text.to_string());
    }
}

/// Whether `script` is the thread this entry was opened for.
///
/// The pointer alone is not enough: a terminated script's address can be handed to the
/// next one, so the name has to agree as well. Where the name comes from depends on which
/// half of the queue the thread is in: CLEO keeps the file name of a script it loaded, and
/// a game thread only has the name `03A4` gave it in its own field - asking CLEO for the
/// file name of one of the game's threads would read a `std::string` that is not there.
fn is_script(entry: &ScriptDebug, script: *mut CRunningScript) -> bool {
    if entry.ptr == 0 || entry.ptr != script as usize {
        return false;
    }

    if entry.kind == Kind::Game {
        return threads::name_matches(unsafe { &(*script).m_abScriptName }, &entry.name);
    }

    let name = unsafe { CLEO_GetScriptFilename(script) };
    if name.is_null() {
        return false;
    }

    unsafe { std::ffi::CStr::from_ptr(name) }.to_str() == Ok(entry.name.as_str())
}

/// Build one trace line from the instruction the callback reported.
///
/// `m_nIp` already points past the 2-byte opcode, which is where the arguments start.
fn record(
    entry: &mut ScriptDebug,
    script: *mut CRunningScript,
    opcode: u16,
) -> Option<trace::Line> {
    let ip = unsafe { (*script).m_nIp as *const u8 };
    // The block, not `m_nBaseIp`: San Andreas leaves that zero on the threads `main.scm`
    // starts, and their `m_nIp` is already an absolute pointer into the block.
    let base = threads::code_base(script) as *const u8;
    if base.is_null() || ip.is_null() {
        return None;
    }

    let offset = unsafe { ip.offset_from(base) };
    if offset < 0 {
        return None;
    }

    // A size of zero means the thread has no code to read, and guessing at one would mean
    // reading past the script.
    let size = entry.code_size;
    let at = offset as usize;
    if size == 0 || at > size {
        return None;
    }

    // Never read past the script: an obfuscated or truncated stream stops here, not in the
    // bytes of whatever follows in memory.
    let args = unsafe { std::slice::from_raw_parts(ip, size - at) };
    let not_flag = unsafe { (*script).m_bNotFlag };
    let prepared = Prepared::new(entry, script, args, opcode);
    // `base_offset`, not the raw relative offset: the SDK counts from the position it was
    // handed, which is already past the opcode.
    let line = prepared.line(script, base_offset(script, ip), opcode, not_flag);
    prepared.teach(&mut entry.local_types);

    Some(line)
}

/// The instruction the script is standing on, i.e. the one about to run.
///
/// Outside a callback `m_nIp` points *at* the opcode with its arguments after it, and the
/// not-flag sits in the opcode word rather than in `m_bNotFlag`. The window shows it as the
/// last trace row, so after a step the highlight is the line that comes next.
pub(in crate::debug) fn upcoming(
    entry: &ScriptDebug,
    script: *mut CRunningScript,
) -> Option<trace::Line> {
    let ip = unsafe { (*script).m_nIp as *const u8 };
    // See [`record`]: the block is the base when the engine did not record one.
    let base = threads::code_base(script) as *const u8;
    if base.is_null() || ip.is_null() {
        return None;
    }

    let offset = unsafe { ip.offset_from(base) };
    if offset < 0 {
        return None;
    }

    let size = entry.code_size;
    let at = offset as usize;
    if size == 0 || at + 2 > size {
        return None;
    }

    let bytes = unsafe { std::slice::from_raw_parts(ip, size - at) };
    let raw = u16::from_le_bytes([bytes[0], bytes[1]]);
    render_line(
        entry,
        script,
        &bytes[2..],
        raw & 0x7FFF,
        at as u32,
        raw & 0x8000 != 0,
    )
}

/// Render one instruction with the trace's layout: offset field, call depth, opcode and the
/// values as they are right now.
fn render_line(
    entry: &ScriptDebug,
    script: *mut CRunningScript,
    args: &[u8],
    opcode: u16,
    offset: u32,
    not_flag: bool,
) -> Option<trace::Line> {
    let prepared = Prepared::new(entry, script, args, opcode);
    Some(prepared.line(script, offset, opcode, not_flag))
}

/// One instruction, decoded and placed in the call it stands in: ready to be rendered, and
/// still carrying the command it came from, which is where its argument types are.
struct Prepared<'a> {
    command: Option<&'a db::Command>,
    instruction: decode::Instruction,
    /// `gosub` and `cleo_call` depth, plus the level a condition list adds.
    level: usize,
}

impl<'a> Prepared<'a> {
    fn new(
        entry: &ScriptDebug,
        script: *mut CRunningScript,
        args: &'a [u8],
        opcode: u16,
    ) -> Self {
        let command = db().and_then(|db| db.get(opcode));
        let instruction = decode::decode(args, plan_for(command));

        let level = unsafe { (*script).m_nStackPointer as usize }
            + unsafe { CLEO_GetCleoCallStackSize(script) } as usize
            + usize::from(continues_condition(entry, script));

        Self {
            command,
            instruction,
            level,
        }
    }

    /// The line twice: with the `{argument}` comments and without, which is what the
    /// `Compact` switch chooses between and why both are recorded.
    fn line(
        &self,
        script: *mut CRunningScript,
        offset: u32,
        opcode: u16,
        not_flag: bool,
    ) -> trace::Line {
        let vars = LiveVars::new(script);

        let expanded = Layout {
            offset,
            opcode,
            not_flag,
            level: self.level,
            compact: false,
            scm_block: scm_block(),
            prefix: true,
            with_values: true,
            labels: LabelStyle::Address,
        };
        let text = render::line(self.command, &self.instruction, &expanded, &vars);

        let short = Layout { compact: true, ..expanded };
        let short = render::line(self.command, &self.instruction, &short, &vars);

        trace::Line {
            text,
            compact: short,
        }
    }

    /// Take the declared type of every local this instruction named.
    ///
    /// A slot holds four bytes and the stream never says what they are, but the command
    /// database says what the instruction reads it as, which is as close to a type as a
    /// script has. Only the instructions that actually ran teach the panel anything, so a
    /// slot shows a type once something has used it.
    fn teach(&self, types: &mut LocalTypes) {
        let command = self.command;
        for (index, value) in self.instruction.inputs.iter().enumerate() {
            types.learn(command.and_then(|command| command.input(index)), value);
        }
        for (index, value) in self.instruction.outputs.iter().enumerate() {
            types.learn(command.and_then(|command| command.output(index)), value);
        }
    }
}

fn plan_for(command: Option<&db::Command>) -> Option<Plan> {
    command.map(|command| Plan {
        inputs: command.inputs.len(),
        varargs_at: command.varargs_at,
        outputs: command.outputs.len(),
    })
}

/// The log's rule for the extra level a condition list carries.
fn continues_condition(entry: &ScriptDebug, script: *mut CRunningScript) -> bool {
    entry.prev_opcode == COMMAND_IF || unsafe { (*script).m_nAndOrState } != 0
}

/// Offset of the opcode within the script, in the log's notation: the SDK counts from the
/// position it is given, which is already past the 2-byte opcode.
fn base_offset(script: *mut CRunningScript, ip: *const u8) -> u32 {
    unsafe { CLEO_GetScriptBaseRelativeOffset(script, ip) }.saturating_sub(2)
}

fn scm_block() -> usize {
    unsafe { CLEO_GetScmMainData() as usize }
}

/// Variable values of a live script: locals from the thread's own 32 slots, globals from the
/// main script block. A read that would fall outside either block is reported as unknown.
struct LiveVars {
    locals: *const u8,
    globals: *const u8,
}

impl LiveVars {
    fn new(script: *mut CRunningScript) -> Self {
        Self {
            locals: unsafe { (*script).m_anLocalVariables.as_ptr() as *const u8 },
            globals: unsafe { CLEO_GetScmMainData() as *const u8 },
        }
    }

    /// The bytes behind a reference: `slots` 4-byte slots of them.
    fn bytes(&self, scope: Scope, index: u16, slots: usize) -> Option<&[u8]> {
        let index = index as usize;

        let (base, limit) = match scope {
            Scope::Local => (self.locals, 32),
            Scope::Global => (self.globals, MAX_GLOBALS),
        };

        if base.is_null() || index + slots > limit {
            return None;
        }

        Some(unsafe { std::slice::from_raw_parts(base.add(index * 4), slots * 4) })
    }
}

impl VarSource for LiveVars {
    fn int(&self, scope: Scope, index: u16) -> Option<i32> {
        let bytes = self.bytes(scope, index, 1)?;
        Some(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn float(&self, scope: Scope, index: u16) -> Option<f32> {
        let bits = self.int(scope, index)?;
        Some(f32::from_bits(bits as u32))
    }

    fn text(&self, scope: Scope, index: u16, kind: VarKind) -> Option<String> {
        let bytes = self.bytes(scope, index, kind.slots())?;
        let used = bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(bytes.len());
        Some(String::from_utf8_lossy(&bytes[..used]).into_owned())
    }
}

/// What a local slot was last seen holding, according to the last instruction that named it.
/// Nothing in a script records a slot's type, so this is a hint; [`SlotType::Unknown`] falls
/// back to guessing from the four bytes.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum SlotType {
    #[default]
    Unknown,
    Int,
    Float,
}

/// The 32 local slots, each with what the trace has learned about it.
#[derive(Clone)]
pub struct LocalTypes([SlotType; 32]);

impl Default for LocalTypes {
    fn default() -> Self {
        Self([SlotType::Unknown; 32])
    }
}

impl LocalTypes {
    /// What slot `index` holds, as far as the trace knows.
    pub fn get(&self, index: usize) -> SlotType {
        self.0.get(index).copied().unwrap_or_default()
    }

    /// Take the declared type of a local that one operand names.
    ///
    /// Only a declared `int` or `float` teaches anything: `any`, a label or a string says
    /// nothing about the four bytes, and an array names its base rather than an element.
    /// A slot two commands disagree about ends up with the last one's opinion, which is the
    /// one just executed.
    fn learn(&mut self, arg: Option<&db::Arg>, value: &decode::Value) {
        let Some(arg) = arg else {
            return;
        };

        let decode::Value::Var {
            scope: Scope::Local,
            index,
            kind: VarKind::Int,
        } = value
        else {
            return;
        };

        let slot = match arg.kind {
            db::ArgKind::Float => SlotType::Float,
            db::ArgKind::Int
            | db::ArgKind::Bool
            | db::ArgKind::Model
            | db::ArgKind::ScriptId
            | db::ArgKind::Other => SlotType::Int,
            db::ArgKind::Any | db::ArgKind::Label | db::ArgKind::String => return,
        };

        if let Some(cell) = self.0.get_mut(usize::from(*index)) {
            *cell = slot;
        }
    }
}

// ----------------------------------------------------------------- panel API

/// Open (or focus) the debug window for a script. `name` is what the panel calls it: CLEO's
/// file name for a script it loaded, the `03A4` name for one of the game's.
pub fn open(key: &str, name: &str, kind: Kind) {
    let Ok(mut state) = state().lock() else {
        return;
    };

    if !state.windows.iter().any(|open| open == key) {
        state.windows.push(key.to_string());
    }

    match state.scripts.iter_mut().find(|script| script.key == key) {
        Some(script) => script.focus = true,
        None => {
            let mut script = ScriptDebug::new(
                key.to_string(),
                name.to_string(),
                kind,
                DEFAULT_LINE_CAP,
            );
            script.focus = true;
            state.scripts.push(script);
        }
    }

    // Parse `sa.json` here, on the UI side, rather than inside the first opcode callback:
    // reading 1.8 MB of JSON on the game thread mid-frame would be a visible hitch.
    let _ = db();
}

/// What the Scripts table badges a row with: is it traced, and is it parked by the panel.
pub fn flags(key: &str) -> (bool, bool) {
    state()
        .lock()
        .map(|state| {
            state
                .scripts
                .iter()
                .find(|script| script.key == key)
                .map(|script| (script.trace, script.control.paused))
                .unwrap_or((false, false))
        })
        .unwrap_or((false, false))
}

/// Point the entry at the current thread for its script, which changes when the script is
/// reloaded. Returns whether a thread was found.
fn refresh_pointer(entry: &mut ScriptDebug) -> bool {
    let Some(thread) = threads::find(&entry.key) else {
        entry.ptr = 0;
        entry.code_size = 0;
        return false;
    };

    entry.ptr = thread.ptr as usize;
    entry.name = thread.name;
    entry.code_size = threads::readable(thread.ptr).unwrap_or(0);
    true
}

// ------------------------------------------------------------------------ UI

/// Draw every open debug window. Called from the overlay's render loop, on the game thread,
/// after the main window.
///
/// `nav` is what the keys asked for this frame. Only the window the user is in acts on it:
/// the sheet underneath stands down while a debugger is on top, so `Tab` and the arrows
/// mean the debugger there instead of the page at the back.
pub fn windows(ui: &Ui, nav: Intent) {
    let Ok(mut state) = state().lock() else {
        return;
    };

    if state.windows.is_empty() {
        // Closing the last window clears the entry list; the callbacks' guards have to
        // follow, or they keep taking the lock for a script nobody watches.
        state.focused = None;
        refresh_active(&state);
        return;
    }

    // Windows are addressed by path; a script that is gone closes its window.
    let keys: Vec<String> = state.windows.clone();
    // Which window the user is in, gathered while they are drawn and stored for the next
    // frame: `Escape` is handled before the windows are, so it reads what was true then.
    let mut focused = None;
    for (ordinal, key) in keys.into_iter().enumerate() {
        let Some(index) = state.scripts.iter().position(|entry| entry.key == key) else {
            state.windows.retain(|open| open != &key);
            continue;
        };

        // The entry is moved out for the duration of the draw so the window code can hold
        // it mutably while the rest of the state stays reachable.
        let mut entry = std::mem::replace(&mut state.scripts[index], placeholder());
        let (keep, has_focus) = window::draw(ui, &mut entry, &mut state, ordinal, nav);
        state.scripts[index] = entry;

        if !keep {
            // Closing the window drops the entry but not the parking: the park is a wake
            // time on the thread itself, so it outlives everything this panel keeps.
            state.scripts.remove(index);
            state.windows.retain(|open| open != &key);
        } else if has_focus {
            focused = Some(key);
        }
    }

    state.focused = focused;
    refresh_active(&state);
}

/// Whether a floating window is the one the user is in, which is where the keyboard goes.
/// `false` puts the sheet back in charge of its own keys.
pub fn has_focus() -> bool {
    state()
        .lock()
        .map(|state| state.focused.is_some())
        .unwrap_or(false)
}

/// Close the focused window the way its own close button would. Returns whether there was
/// one, so the caller can fall back to the sheet: `Escape` backs out of one window at a time.
/// A parked script stays parked, exactly as with the close button.
pub fn close_focused() -> bool {
    let Ok(mut state) = state().lock() else {
        return false;
    };

    let Some(key) = state.focused.take() else {
        return false;
    };

    state.scripts.retain(|script| script.key != key);
    state.windows.retain(|open| open != &key);

    // The focus ImGui held died with the window: hand it to one that is left, so the next
    // Escape backs out of that instead of falling through to the sheet.
    let next = state.windows.last().cloned();
    if let Some(next) = next {
        if let Some(script) = state.scripts.iter_mut().find(|script| script.key == next) {
            script.focus = true;
        }
    }

    refresh_active(&state);
    true
}

/// A stand-in the panel swaps in while a window borrows its entry.
fn placeholder() -> ScriptDebug {
    ScriptDebug::new(String::new(), String::new(), Kind::Cleo, trace::MIN_CAPACITY)
}
