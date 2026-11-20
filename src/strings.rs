//! Every string in the menu, in one place.

/// `Yes`/`No` for a flag row or a boolean cell.
pub fn yes_no(value: bool) -> &'static str {
    if value {
        "Yes"
    } else {
        "No"
    }
}

// ------------------------------------------------------------------ window chrome

pub mod tabs {
    pub const SCRIPTS: &str = "Scripts";
    pub const LIBRARIES: &str = "Libraries";
    pub const AUDIO: &str = "Audio";
    pub const LOGS: &str = "Logs";
    pub const SETTINGS: &str = "Settings";
}

pub mod title_bar {
    pub const NAME: &str = "CLEO Dashboard";
    pub const LOGO: &str = "CD";
}

pub mod keys {
    pub const PAUSE: &str = "[F5]";
    pub const STEP: &str = "[F10]";
    pub const FIELDS: &str = "[F]";
}

pub mod highlight {
    pub const LABEL: &str = "Syntax Highlight";
    pub const TIP: &str = "Highlights syntax elements in logs and traces.";
}

pub mod footer {
    pub const FPS_LABEL: &str = "FPS:";

    pub const CHECK_UPDATES: &str = "Check Updates";
    pub const CHECKING: &str = "Checking...";
    pub const UPDATE_AVAILABLE: &str = "Update available!";
    pub const UP_TO_DATE: &str = "Up to date";

    /// The footer line of a table page: `12 scripts displayed`.
    pub fn shown(count: usize, singular: &str, plural: &str) -> String {
        match count {
            1 => format!("1 {singular} displayed"),
            _ => format!("{count} {plural} displayed"),
        }
    }

    /// The footer line of the Logs tab: which of the two files is on screen.
    pub fn log_stream(path: &str) -> String {
        format!("Streaming {path}")
    }

    /// The footer line of the Settings tab: the ini the page writes.
    pub fn settings_file(path: &str) -> String {
        format!("Settings file: {path}")
    }

    /// Columns the script table dropped for want of room.
    pub fn hidden_columns(columns: &str) -> String {
        format!("  \u{00B7}  {columns} hidden: no room")
    }

    /// The quick-filter hint under the page summary: the number keys pick a chip.
    pub fn quick_filter(count: usize) -> String {
        format!("  \u{00B7}  Press 1-{count} to filter")
    }

    pub const PRESS_SPACE_PAUSE: &str = "  \u{00B7}  Press Space to Pause";
    pub const PRESS_SPACE_RESUME: &str = "  \u{00B7}  Press Space to Resume";

    pub const CLOSE_HINT_BEFORE: &str = "Press ";
    pub const CLOSE_HINT_MIDDLE: &str = " or ";
    pub const CLOSE_HINT_ESCAPE: &str = "Esc";
    pub const CLOSE_HINT_AFTER: &str = " to close the menu  |  Change the menu hotkey in ";
    pub const SETTINGS_LINK: &str = "Settings";
    pub const TIP_SETTINGS_LINK: &str = "Open the Settings tab";
}

// --------------------------------------------------------------- tables, shared

pub mod table {
    /// Row number, shared by every table.
    pub const HEADER_INDEX: &str = "#";
}

pub mod scripts {
    pub const HEADER_SCRIPT: &str = "Script";
    pub const HEADER_SIZE: &str = "Size";
    pub const HEADER_MODE: &str = "Mode";
    pub const HEADER_STATUS: &str = "Status";
    pub const HEADER_MEMORY: &str = "Memory";

    pub const STATUS_ACTIVE: &str = "Active";
    pub const STATUS_SUSPENDED: &str = "Suspended";

    /// The filter chips above the table.
    pub const CHIP_ALL: &str = "All";
    pub const CHIP_ACTIVE: &str = "Active";
    pub const CHIP_SUSPENDED: &str = "Suspended";
    pub const CHIP_CUSTOM: &str = "Custom";
    pub const CHIP_GAME: &str = "Game";

    /// The column picker beside the chips.
    pub const COLUMNS: &str = "Columns";

    pub const FILTER_PLACEHOLDER: &str = "Filter scripts...  (/ to focus)";

    pub const COMPAT_5: &str = "CLEO 5";
    pub const COMPAT_4: &str = "CLEO 4";
    pub const COMPAT_3: &str = "CLEO 3";
    /// A script version that predates CLEO 3 has no mode to map onto.
    pub const COMPAT_UNKNOWN: &str = "Unknown";
    /// Note beside the compat combo while the picked mode is not the loaded one.
    pub const RELOAD_REQUIRED: &str = "Reload required";

    /// Size cell of a script whose bytecode could not be bounded at all.
    pub const SIZE_UNKNOWN: &str = "-";

    /// Memory cell of a script with no allocations, and its tooltip.
    pub const MEMORY_NONE: &str = "-";
    pub const TIP_MEMORY_NONE: &str = "This script has not allocated any memory yet.";

    pub const TIP_COMPAT: &str = "Affects opcode behavior and error handling.\nPress Reload button to restart this script in the selected mode.";
    pub const TIP_COMPAT_NATIVE: &str =
        "Affects opcode behavior and error handling for every script the game started.\n\
         CLEO keeps one mode for all of them, so this changes them together, at once.";
    pub const TIP_STATUS: &str =
        "Green: the script is running, and the yellow bar is what a click does - park it.\n\
         Yellow: it is parked with its state kept, and the green arrow resumes it.";
    pub const TIP_CHIP_CUSTOM: &str = "Scripts CLEO loaded from a .cs file";
    pub const TIP_CHIP_GAME: &str = "Scripts the game started itself, from main.scm or script.img.";
    pub const TIP_SELECT: &str =
        "Click a row's number or name to show that script's state on the right.";
    pub const TIP_PAUSE: &str =
        "Pauses the script. Its state and variables are preserved.\nIt can be resumed from where it stopped.";
    pub const TIP_RESUME: &str = "Lets the parked script run again, from where it stopped.";
    pub const TIP_SUSPEND_LOCKED: &str =
        "A mission script: parking it mid-mission leaves the mission state inconsistent.\n\
         Set Dashboard.Debug.AllowNativeControl=1 to allow it.";
    pub const TIP_MEMORY_NATIVE: &str =
        "The game's own scripts allocate through the engine's pools, which CLEO does not count.";
    pub const TIP_SIZE_UNKNOWN_NATIVE: &str = "The engine records no start for this script: the\n\
         scripts main.scm starts carry only a current position, so there is no size to\n\
         measure. A mission from script.img is measured from the mission table in the SCM\n\
         header instead.";

    pub fn size_tooltip(bytes: usize) -> String {
        format!("{bytes} bytes")
    }

    /// Size of a game script: a measurement, not a length it carries.
    pub fn native_size_tooltip(bytes: usize) -> String {
        format!(
            "~{bytes} bytes\nThe game does not record a length per script. A mission out of\n\
             script.img is measured from the mission table in main.scm's header, and a script\n\
             another script or a plugin placed is measured to the next script in its block."
        )
    }

    /// Memory cell: `count/max_count · size`.
    pub fn memory_cell(count: usize, max_count: usize, size: &str) -> String {
        format!("{count}/{max_count} · {size}")
    }

    /// The same numbers, exact, with their share of the limits.
    pub fn memory_tooltip(
        count: usize,
        max_count: usize,
        bytes: usize,
        max_bytes: usize,
    ) -> String {
        use super::pct;

        format!(
            "{count}/{max_count} chunks ({:.2}%)\n\
             {bytes}/{max_bytes} bytes ({:.2}%)",
            pct(count, max_count),
            pct(bytes, max_bytes),
        )
    }
}

/// The Script Info panel beside the script table.
pub mod sidebar {
    pub const HEAD: &str = "Script Info";
    pub const FIELDS: &str = "Script Flags";
    pub const DEBUG_MODE: &str = "Debug mode";
    pub const DEBUGGER: &str = "Debugger";
    pub const DUMP: &str = "Dump";
    pub const RELOAD: &str = "Reload";
    /// What the Script name row says for a script that never ran `03A4`.
    pub const NO_SCRIPT_NAME: &str = "unnamed";
    pub const FIELD_FILE_NAME: &str = "File name";
    pub const FIELD_SCRIPT_NAME: &str = "Script name";

    pub const TIP_DUMP: &str =
        "Writes this script's bytecode to <name>.dump next to the .cs file.\nNot offered for a\n\
         script the game started: it is not a file, and records no start to copy from.";
    pub const TIP_RELOAD: &str =
        "Restarts the script, which is also how a changed compat mode is applied.";

    pub const TIP_NAME: &str = "The file the script was loaded from.";
    pub const TIP_NAME_NATIVE: &str =
        "The file the game's script came from: main.scm, or script.img for a mission.";
    pub const TIP_SCRIPT_NAME: &str =
        "The name the script gave itself with `03A4: name_script`, which CLEO's own scripts set too.";
    pub const TIP_OPEN_POINTER: &str = "Opens this script in the debugger.";
    pub const TIP_CLOSE: &str = "Closes this panel. The script keeps running.";
    pub const TIP_DEBUG_MODE: &str =
        "Enable debug features for this script, like on-screen trace logs and breakpoints.";
    pub const TIP_OPEN_DEBUGGER: &str =
        "Opens the script's live tracing, flags, and variables, with Pause and Step.";
}

pub mod libraries {
    /// Nothing in the process matches the chip that is up.
    pub const EMPTY: &str = "No modules match this filter.";
    /// The loader list could not be read at all.
    pub const UNAVAILABLE: &str = "Could not read the loaded modules.";

    pub const HEADER_MODULE: &str = "Module";
    /// A reference count: how often a script asked CLEO for the library. Only the modules
    /// CLEO's own bookkeeping knows get one.
    pub const HEADER_LOADS: &str = "Times loaded";
    pub const HEADER_BASE: &str = "Base";
    /// A module CLEO never loaded for a script, so it has no count to show.
    pub const NO_LOADS: &str = "-";

    /// The filter chips above the table. They narrow by file extension.
    pub const CHIP_ALL: &str = "All";
    pub const CHIP_CLEO: &str = "CLEO";
    pub const CHIP_DLL: &str = "DLL";
    pub const CHIP_ASI: &str = "ASI";

    pub const TIP_CHIP_CLEO: &str = "Modules loaded by CLEO, i.e. its .cleo plugins";
    pub const TIP_CHIP_DLL: &str =
        "Plain .dll modules: the game's, the system's, and mods that ship one";
    pub const TIP_CHIP_ASI: &str = "ASI mods, loaded by an ASI loader";

    pub const FILTER_PLACEHOLDER: &str = "Filter modules...  (/ to focus)";

    /// The switch beside the chips: system DLLs are most of the list and rarely what
    /// anyone came for.
    pub const SHOW_SYSTEM: &str = "Show system libs";
    pub const TIP_SHOW_SYSTEM: &str =
        "Also list the modules Windows loads: System32, WinSxS and the DriverStore";
}

pub mod audio {
    pub const EMPTY: &str = "No audio streams loaded.";

    pub const HEADER_FILENAME: &str = "Filename";
    pub const HEADER_TYPE: &str = "Type";
    pub const HEADER_LENGTH: &str = "Length";
    pub const HEADER_STATE: &str = "State";
    pub const HEADER_3D: &str = "3D";
    pub const HEADER_LOOP: &str = "Loop";
    pub const HEADER_VOLUME: &str = "Volume";
    pub const HEADER_SPEED: &str = "Speed";

    pub const TYPE_NONE: &str = "None";
    pub const TYPE_SFX: &str = "SFX";
    pub const TYPE_MUSIC: &str = "Music";
    pub const TYPE_UI: &str = "UI";
    pub const TYPE_UNKNOWN: &str = "Unknown";

    pub const STATE_STOPPED: &str = "Stopped";
    pub const STATE_INACTIVE: &str = "Inactive";
    pub const STATE_PLAYING: &str = "Playing";
    pub const STATE_PAUSED: &str = "Paused";
    pub const STATE_UNKNOWN: &str = "Unknown";

    pub const SECONDS: &str = "sec";
}

pub mod logs {
    pub const SOURCE_CLEO: &str = "cleo.log";
    pub const TIP_SOURCE_CLEO: &str =
        "CLEO 5 working log: plugin info, script lifecycle, warnings and errors.";
    pub const SOURCE_SCRIPT: &str = "cleo_script.log";
    pub const TIP_SOURCE_SCRIPT: &str = "Opcode execution log, managed by the DebugUtils plugin.";

    pub const AUTO_SCROLL: &str = "Auto-scroll";
    pub const TIP_AUTO_SCROLL: &str = "Keeps the newest line in view. Scrolling the log manually turns it off and freezes the view.";
    pub const JUMP_TO_LATEST: &str = "Jump to latest";
    pub const TIP_JUMP_TO_LATEST: &str =
        "Scrolls to the newest line and switches Auto-scroll back on.";

    pub const TRACING_LABEL: &str = "Tracing:";
    pub const TRACING_OFF: &str = "Off";
    pub const TRACING_ON_CRASH: &str = "On crash";
    pub const TRACING_FULL: &str = "Full";
    pub const TIP_TRACING: &str = "Tracing level:\n\
        Off - don't log any opcodes.\n\
        On crash - keep only opcodes executed in the most recent frame.\n\
        Full - keep all opcodes from the start of the game (slow).";

    /// The toolbar button that opens the script log's own settings.
    pub const TIP_CONFIG: &str = "Configure script log capture options and output style.";

    pub const LOG_MAX_SIZE: &str = "Max size (MB)";
    pub const TIP_LOG_MAX_SIZE: &str =
        "The log is cleared when it reaches this size. 0 keeps it unlimited.";
    pub const LOG_CUSTOM_SCRIPTS: &str = "Custom scripts";
    pub const TIP_LOG_CUSTOM_SCRIPTS: &str =
        "Log only custom scripts and skip main.scm. Automatic decides from the game state.";
    pub const LOG_SCOPE_AUTO: &str = "Automatic";
    pub const LOG_SCOPE_ALL: &str = "All scripts";
    pub const LOG_SCOPE_CUSTOM: &str = "Custom only";
    pub const LOG_DEBUG_SCRIPTS: &str = "Debug scripts";
    pub const TIP_LOG_DEBUG_SCRIPTS: &str = "Log only the scripts that ran debug_on.";
    pub const LOG_OFFSETS: &str = "Offsets";
    pub const TIP_LOG_OFFSETS: &str = "Write the code offset of every instruction.";
    pub const LOG_OPCODES: &str = "Opcodes";
    pub const TIP_LOG_OPCODES: &str = "Write the opcode number in front of each command.";
    /// Said once at the top of the popup, since none of the rows would be obvious on its
    /// own: the plugin only reads these when its logging starts.
    pub const LOG_CONFIG_NOTE: &str = "Changing these options restarts the log.";

    /// The plugin that writes `cleo_script.log` and owns the tracing level. Without it the
    /// level is a setting nothing reads, so the dropdown is offered disabled.
    pub const TRACING_PLUGIN: &str = "SA.DebugUtils.cleo";
    /// Said where the disabled dropdown is.
    pub const TRACING_NO_PLUGIN: &str = "SA.DebugUtils.cleo not loaded";

    pub const SEARCH_PLACEHOLDER: &str = "Search...  (/ to focus)";
    pub const TIP_SEARCH: &str = "Shows only the lines containing this text. Case does not matter.";
    /// Where the filter leaves nothing at all.
    pub const NO_MATCH: &str = "no line matches the filter";
    /// How much of the log the filter is showing, drawn after the toolbar controls.
    pub fn matching(shown: usize, total: usize) -> String {
        format!("{shown} of {total} lines")
    }

    pub const EMPTY: &str = "(empty)";

    /// The script log is missing because tracing is off.
    pub const NO_LOG_YET: &str = "no log yet: pick Tracing \"On crash\" or \"Full\" below";

    /// The file itself is not there.
    pub fn not_written(path: &str) -> String {
        format!("{path} not written yet")
    }
}

pub mod settings {
    pub const SECTION_APPEARANCE: &str = "Appearance";
    pub const SECTION_BEHAVIOUR: &str = "Behavior";

    /// Theme names, in the order of `theme::Theme`.
    pub const THEME_NAMES: [&str; 3] = ["Blue", "Purple", "Orange"];

    pub const THEME: &str = "Theme";
    pub const TIP_THEME: &str = "Look of the menu. Applied immediately, saved when you pick one.";

    pub const TOGGLE_KEY: &str = "Toggle key";
    pub const TIP_TOGGLE_KEY: &str =
        "Key that opens and closes this menu. Changes apply immediately.";

    pub const UI_SCALE: &str = "UI scale";
    pub const TIP_UI_SCALE: &str = "Scale of the menu text and widgets (0.5 - 2.0).";

    pub const MENU_OPACITY: &str = "Menu opacity";
    pub const TIP_MENU_OPACITY: &str = "Background transparency of this menu (0.15 - 1.0). \
        Applied while dragging, saved when you let go.";

    pub const FREEZE_INPUT: &str = "Freeze game input";
    pub const TIP_FREEZE_INPUT: &str =
        "Ignore any input while the menu is open, so the player and camera stay still.";

    pub const RELEASE_CURSOR: &str = "Release mouse cursor";
    pub const TIP_RELEASE_CURSOR: &str = "Hides the game's cursor.";

    pub const SHOW_FPS: &str = "Show FPS";
    pub const TIP_SHOW_FPS: &str = "Shows the frame rate in the footer.";

    pub const OPEN_ON_START: &str = "Open on start";
    pub const TIP_OPEN_ON_START: &str = "Shows the menu as soon as the overlay hooks in.";
}

pub mod debug {
    pub const WINDOW: &str = "Script Debugger";

    // --------------------------------------------------- window layout
    pub const PANE_DISASM: &str = "Disassembly";
    pub const PANE_TRACE: &str = "Trace";

    pub const LISTING_EMPTY: &str = "Nothing to list.";
    pub const LISTING_CAPPED: &str = "listing stops after 20 000 instructions";
    pub const LISTING_BROKEN: &str = "listing stops here: not a valid instruction";

    // --------------------------------------------------- variables
    pub const VARIABLES: &str = "Variables";
    pub const TIMER_A: &str = "TIMERA";
    pub const TIMER_B: &str = "TIMERB";

    // --------------------------------------------------- flags
    pub const FIELDS: &str = "Script Flags";
    pub const STATE_FLAGS: &str = "State Flags";
    pub const ADDITIONAL_INFO: &str = "Additional Info";

    pub const FIELD_SCRIPT_NAME: &str = "Script Name";
    pub const FIELD_NEXT: &str = "Next";
    pub const FIELD_PREVIOUS: &str = "Previous";
    pub const FIELD_BASE_IP: &str = "BaseIP";
    pub const FIELD_CURRENT_IP: &str = "CurrentIP";
    pub const FIELD_SP: &str = "SP";
    pub const FIELD_WAKE_TIME: &str = "Wake Time";

    // The captions say what the byte does, not what it is called in the engine. The
    // tooltips give the engine name, so a reader can still find it in the struct.
    pub const FLAG_ACTIVE: &str = "m_bIsActive";
    pub const TIP_FLAG_ACTIVE: &str = "m_bIsActive: this script is running";
    pub const FLAG_COND: &str = "m_bCondResult";
    pub const TIP_FLAG_COND: &str = "m_bCondResult: result of the last conditional opcode";
    pub const FLAG_CLEANUP: &str = "m_bUseMissionCleanup";
    pub const TIP_FLAG_CLEANUP: &str =
        "m_bUseMissionCleanup: this script tracks entities for automatic cleanup";
    pub const FLAG_EXTERNAL: &str = "m_bIsExternal";
    pub const TIP_FLAG_EXTERNAL: &str = "m_bIsExternal: a script from script.img";
    pub const FLAG_TEXT_OVERRIDE: &str = "m_bTextBlockOverride";
    pub const TIP_FLAG_TEXT_OVERRIDE: &str = "m_bTextBlockOverride";
    pub const FLAG_MISSION: &str = "m_bIsMission";
    pub const TIP_FLAG_MISSION: &str =
        "m_bIsMission: a mission script, enables special behavior for some opcodes";
    pub const FLAG_CUSTOM: &str = "m_bIsCustom";
    pub const TIP_FLAG_CUSTOM: &str = "m_bIsCustom: a CLEO script";
    pub const FLAG_WASTED_CHECK: &str = "m_bWastedBustedCheck";
    pub const TIP_FLAG_WASTED_CHECK: &str =
        "m_bWastedBustedCheck: unwinds gosub stack on arrest/death";
    pub const FLAG_WASTED: &str = "m_bWastedOrBusted";
    pub const TIP_FLAG_WASTED: &str = "m_bWastedOrBusted: arrested or dead";

    pub const INFO_EXTERNAL_TYPE: &str = "External Type";
    pub const INFO_LOGICAL_OP: &str = "Logical Op";
    pub const INFO_NOT_FLAG: &str = "NotFlag";
    pub const INFO_SCM_FUNCTION: &str = "ScmFunction";

    pub const TIP_FIELDS: &str = "The script's raw fields and state flags. Read-only. F";
    pub const TIP_STATE_FLAGS: &str = "Read-only: they are what the game has right now.";

    /// `Variables (32)`.
    pub fn variables(count: usize) -> String {
        format!("{VARIABLES} ({count})")
    }

    /// `[ 7] = `, the local grid's caption for slot 7. The value beside it is drawn in the
    /// text color, the way every reading in the menu is.
    pub fn local_label(index: usize) -> String {
        format!("[{index:2}] = ")
    }

    /// `TIMERA = `, a timer's caption. The number beside it is drawn in the text color.
    pub fn timer_label(name: &str) -> String {
        format!("{name} = ")
    }

    /// `1234`, a timer's reading. The `TIMERA = ` before it is a label of its own.
    pub fn timer_value(value: i32) -> String {
        value.to_string()
    }

    /// What the window says when the script it was opened for is gone.
    pub const GONE: &str = "This script is no longer running.";
    pub const CLOSE: &str = "Close";
    pub const EMPTY: &str = "Nothing recorded yet.";

    pub const STEP: &str = "Step";
    pub const PAUSE: &str = "Pause";
    pub const RESUME: &str = "Resume";
    pub const PAUSED: &str = "paused";
    /// Badge beside a script's path in the table while its trace is on.
    pub const BADGE_TRACING: &str = "tracing";

    pub const TIP_PAUSE: &str = "Stops the script. F5";
    pub const TIP_RESUME: &str = "Lets the script run again. F5";
    pub const TIP_STEP: &str = "Runs next instruction, then stops. F10";
    /// Why the two controls are greyed out: a mission script is the one script they refuse.
    pub const TIP_LOCKED: &str =
        "A mission script: parking it mid-mission leaves the mission state inconsistent.\n\
         Set Dashboard.Debug.AllowNativeControl=1 to allow it.";

    pub const AUTO_SCROLL: &str = "Auto-scroll";
    pub const TIP_AUTO_SCROLL: &str = "Keeps the active line in view. A turn of the wheel, \
         either direction, turns it off.";
    pub const COMPACT: &str = "Compact";
    pub const TIP_COMPACT: &str =
        "Offset, opcode and values only: no command name, no indentation.";
    pub const TIP_CLEAR: &str = "Clears the recorded lines.";
    pub const TIP_COPY: &str = "Copies the lines shown to the clipboard.";
    pub const TIP_MORE: &str = "How the lines are written.";
    /// What the count beside the buttons means: recorded lines out of the cap.
    pub const TIP_LINE_COUNT: &str =
        "Instructions recorded so far, against the cap on how many are kept. The oldest are \
         dropped once it is reached.";
    /// The same count on the disassembly, where the lines are the script rather than a
    /// recording and there is no cap to show.
    pub const TIP_LISTING_COUNT: &str =
        "Instructions in this script: the whole listing, all of it kept. Nothing is recorded \
         and nothing is dropped.";

    // What a local holds: a slot the trace has seen used says what the command that used
    // it reads it as, and `window::local_hint` only guesses for the rest.
    pub const HINT_ZERO: &str = "zero";
    pub const HINT_INT: &str = "looks like an integer";
    pub const HINT_FLOAT: &str = "looks like a float";
    pub const HINT_POINTER: &str = "looks like an address";
    pub const HINT_TYPED_INT: &str = "an integer, from the instruction that last used it";
    pub const HINT_TYPED_FLOAT: &str = "a float, from the instruction that last used it";

    /// `gosub stack: 0x00401234 > 0x00401A2B`.
    pub fn gosub_stack(stack: &str) -> String {
        format!("gosub stack: {stack}")
    }

    /// `40/2000`: the lines recorded, and the most that are kept.
    pub fn line_count(count: usize, cap: usize) -> String {
        format!("{count}/{cap}")
    }

    /// `1420 lines`: what the disassembly shows, which has no cap to count against.
    pub fn listing_count(count: usize) -> String {
        format!("{count} lines")
    }

    /// `... 12 earlier lines`.
    pub fn earlier(count: usize) -> String {
        format!("\u{2026} {count} earlier lines")
    }

    /// The message CLEO logged when it suspended the script.
    pub fn suspended_reason(summary: &str) -> String {
        format!("suspended: {summary}")
    }
}

// --------------------------------------------------------------- text formatting

/// Pretty size: `84 B`, `5.7 KB`, `3.03 MB`.
pub fn pretty_size(bytes: usize) -> String {
    const KIB: f64 = 1024.0;
    let value = bytes as f64;
    if value < KIB {
        format!("{bytes} B")
    } else if value < KIB * KIB {
        format!("{:.1} KB", value / KIB)
    } else {
        format!("{:.2} MB", value / (KIB * KIB))
    }
}

/// `value` as a percentage of `limit`, or `0.0` when the limit is zero.
pub fn pct(value: usize, limit: usize) -> f64 {
    if limit == 0 {
        0.0
    } else {
        value as f64 / limit as f64 * 100.0
    }
}
