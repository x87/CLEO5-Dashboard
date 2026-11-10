//! Every string in the menu, in one place.
// ------------------------------------------------------------------ window chrome

pub mod tabs {
    pub const SCRIPTS: &str = "Scripts";
    pub const LIBRARIES: &str = "Libraries";
    pub const AUDIO: &str = "Audio";
    pub const LOGS: &str = "Logs";
    pub const SETTINGS: &str = "Settings";
}

pub mod footer {
    pub const VERSION_LABEL: &str = "CLEO Version:";
    pub const FPS_LABEL: &str = "FPS:";

    pub const CHECK_UPDATES: &str = "Check Updates";
    pub const CHECKING: &str = "Checking...";
    pub const UPDATE_AVAILABLE: &str = "Update available!";
    pub const UP_TO_DATE: &str = "Up to date";

    /// Second footer line; `key` is the live label of the menu hotkey.
    pub fn close_hint(key: &str) -> String {
        format!("Press {key} to close the menu  |  Change the menu hotkey in Settings")
    }
}

// --------------------------------------------------------------- tables, shared

pub mod table {
    /// Row number, shared by every table.
    pub const HEADER_INDEX: &str = "#";
}

pub mod scripts {
    pub const HEADER_SCRIPT: &str = "Script";
    pub const HEADER_SIZE: &str = "Size";
    pub const HEADER_COMPAT: &str = "Compat";
    pub const HEADER_DEBUG: &str = "Debug";
    pub const HEADER_SUSPENDED: &str = "Suspended";
    pub const HEADER_MEMORY: &str = "Memory";
    pub const HEADER_ACTIONS: &str = "Actions";

    pub const FILTER_PLACEHOLDER: &str = "Filter...";

    pub const BUTTON_DUMP: &str = "Dump";
    pub const BUTTON_RELOAD: &str = "Reload";

    pub const COMPAT_5: &str = "CLEO 5";
    pub const COMPAT_4: &str = "CLEO 4";
    pub const COMPAT_3: &str = "CLEO 3";
    /// A script version that predates CLEO 3 has no mode to map onto.
    pub const COMPAT_UNKNOWN: &str = "Unknown";
    /// Note beside the compat combo while the picked mode is not the loaded one.
    pub const RELOAD_REQUIRED: &str = "Reload required";

    /// Memory cell of a script with no allocations, and its tooltip.
    pub const MEMORY_NONE: &str = "-";
    pub const TIP_MEMORY_NONE: &str = "This script has not allocated any memory yet.";

    pub const TIP_COMPAT: &str = "Affects opcode behavior and error handling.\nPress Reload button to restart this script in the selected mode.";
    pub const TIP_DEBUG: &str = "Enables tracing, breakpoints, and execution log";
    pub const TIP_SUSPENDED: &str =
        "Pauses the script. Its state and variables are preserved.\nIt can be resumed from where it stopped.";
    pub const TIP_DUMP: &str = "Writes this script's bytecode to <name>.dump next to the .cs file.";
    pub const TIP_RELOAD: &str = "Restarts the script. Also applies the selected compat mode.";

    pub fn size_tooltip(bytes: usize) -> String {
        format!("{bytes} bytes")
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

pub mod libraries {
    pub const EMPTY: &str = "No libraries loaded.";

    pub const HEADER_LIBRARY: &str = "Library";
    /// A reference count: how often a script asked for the library.
    pub const HEADER_LOADS: &str = "Times loaded";
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

    pub const YES: &str = "Yes";
    pub const NO: &str = "No";

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
    pub const FROZEN: &str = "frozen";

    pub const TRACING_LABEL: &str = "Tracing:";
    pub const TRACING_OFF: &str = "Off";
    pub const TRACING_ON_CRASH: &str = "On crash";
    pub const TRACING_FULL: &str = "Full";
    pub const TIP_TRACING: &str = "Tracing level:\n\
        Off - don't log any opcodes.\n\
        On crash - keep only opcodes executed in the most recent frame.\n\
        Full - keep all opcodes from the start of the game (slow).";

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
    pub const SECTION_STORAGE: &str = "Storage";

    pub const LABEL_SETTINGS_FILE: &str = "Settings file";

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
