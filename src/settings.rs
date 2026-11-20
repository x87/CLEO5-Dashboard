//! Overlay settings and the descriptor table that renders them.
//!
//! Values live in `.cleo_config.ini` (`[Plugins]` section) through the CLEO config API, so
//! they survive restarts and can be edited by hand:
//!
//! ```ini
//! [Plugins]
//! Dashboard.General.ToggleKey=45
//! Dashboard.General.UiScale=1.0
//! ```

use std::ffi::CString;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use imgui::Ui;

use crate::logtail;
use crate::overlay;
use crate::sdk::*;
use crate::strings;
use crate::theme;

const K_THEME: &str = "Dashboard.General.Theme";
const K_TOGGLE_KEY: &str = "Dashboard.General.ToggleKey";
const K_UI_SCALE: &str = "Dashboard.General.UiScale";
const K_MENU_OPACITY: &str = "Dashboard.General.MenuOpacity";
const K_FREEZE_INPUT: &str = "Dashboard.General.FreezeInput";
const K_RELEASE_CURSOR: &str = "Dashboard.General.ReleaseCursor";
const K_SHOW_FPS: &str = "Dashboard.General.ShowFps";
const K_OPEN_ON_START: &str = "Dashboard.General.OpenOnStart";
const K_HIGHLIGHT: &str = "Dashboard.General.Highlight";
/// Which log the Logs tab shows. Not in [`SCHEMA`]: it is UI state, remembered without a row
/// on the Settings page.
const K_LOG_SOURCE: &str = "Dashboard.General.LogSource";

/// The key the menu opens and closes on out of the box. Backtick is the one key the
/// alphabet, the digits and the punctuation row do without.
pub const DEFAULT_TOGGLE_KEY: u8 = 0xC0;

/// One entry of the hotkey dropdown; the value is a virtual-key code.
pub struct ToggleKey {
    pub label: &'static str,
    pub vk: u8,
}

/// Every key the US layout has that is safe to grab: the alphabet, the digits, the
/// punctuation row, F1-F12, CapsLock and the navigation cluster. `Esc` is left out on
/// purpose (the game always needs it), as are the arrows and the numpad.
pub const TOGGLE_KEYS: &[ToggleKey] = &[
    // navigation cluster and locks
    ToggleKey { label: "Insert", vk: 0x2D },
    ToggleKey { label: "Delete", vk: 0x2E },
    ToggleKey { label: "Home", vk: 0x24 },
    ToggleKey { label: "End", vk: 0x23 },
    ToggleKey { label: "PageUp", vk: 0x21 },
    ToggleKey { label: "PageDown", vk: 0x22 },
    ToggleKey { label: "CapsLock", vk: 0x14 },
    // function row
    ToggleKey { label: "F1", vk: 0x70 },
    ToggleKey { label: "F2", vk: 0x71 },
    ToggleKey { label: "F3", vk: 0x72 },
    ToggleKey { label: "F4", vk: 0x73 },
    ToggleKey { label: "F5", vk: 0x74 },
    ToggleKey { label: "F6", vk: 0x75 },
    ToggleKey { label: "F7", vk: 0x76 },
    ToggleKey { label: "F8", vk: 0x77 },
    ToggleKey { label: "F9", vk: 0x78 },
    ToggleKey { label: "F10", vk: 0x79 },
    ToggleKey { label: "F11", vk: 0x7A },
    ToggleKey { label: "F12", vk: 0x7B },
    // digits, top row only
    ToggleKey { label: "0", vk: 0x30 },
    ToggleKey { label: "1", vk: 0x31 },
    ToggleKey { label: "2", vk: 0x32 },
    ToggleKey { label: "3", vk: 0x33 },
    ToggleKey { label: "4", vk: 0x34 },
    ToggleKey { label: "5", vk: 0x35 },
    ToggleKey { label: "6", vk: 0x36 },
    ToggleKey { label: "7", vk: 0x37 },
    ToggleKey { label: "8", vk: 0x38 },
    ToggleKey { label: "9", vk: 0x39 },
    // letters
    ToggleKey { label: "A", vk: 0x41 },
    ToggleKey { label: "B", vk: 0x42 },
    ToggleKey { label: "C", vk: 0x43 },
    ToggleKey { label: "D", vk: 0x44 },
    ToggleKey { label: "E", vk: 0x45 },
    ToggleKey { label: "F", vk: 0x46 },
    ToggleKey { label: "G", vk: 0x47 },
    ToggleKey { label: "H", vk: 0x48 },
    ToggleKey { label: "I", vk: 0x49 },
    ToggleKey { label: "J", vk: 0x4A },
    ToggleKey { label: "K", vk: 0x4B },
    ToggleKey { label: "L", vk: 0x4C },
    ToggleKey { label: "M", vk: 0x4D },
    ToggleKey { label: "N", vk: 0x4E },
    ToggleKey { label: "O", vk: 0x4F },
    ToggleKey { label: "P", vk: 0x50 },
    ToggleKey { label: "Q", vk: 0x51 },
    ToggleKey { label: "R", vk: 0x52 },
    ToggleKey { label: "S", vk: 0x53 },
    ToggleKey { label: "T", vk: 0x54 },
    ToggleKey { label: "U", vk: 0x55 },
    ToggleKey { label: "V", vk: 0x56 },
    ToggleKey { label: "W", vk: 0x57 },
    ToggleKey { label: "X", vk: 0x58 },
    ToggleKey { label: "Y", vk: 0x59 },
    ToggleKey { label: "Z", vk: 0x5A },
    // punctuation, in US layout order
    ToggleKey { label: "` (backtick)", vk: 0xC0 },
    ToggleKey { label: "- (minus)", vk: 0xBD },
    ToggleKey { label: "= (equals)", vk: 0xBB },
    ToggleKey { label: "[ (left bracket)", vk: 0xDB },
    ToggleKey { label: "] (right bracket)", vk: 0xDD },
    ToggleKey { label: "\\ (backslash)", vk: 0xDC },
    ToggleKey { label: "; (semicolon)", vk: 0xBA },
    ToggleKey { label: "' (quote)", vk: 0xDE },
    ToggleKey { label: ", (comma)", vk: 0xBC },
    ToggleKey { label: ". (period)", vk: 0xBE },
    ToggleKey { label: "/ (slash)", vk: 0xBF },
];

#[derive(Clone, Copy, PartialEq)]
pub struct Settings {
    /// Index into `theme::Theme::ALL`.
    pub theme: i32,
    pub toggle_key: i32,
    pub ui_scale: f32,
    pub menu_opacity: f32,
    pub freeze_input: bool,
    pub release_cursor: bool,
    pub show_fps: bool,
    pub open_on_start: bool,
    /// Colour log and trace lines by what their parts are.
    pub highlight: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: theme::Theme::DEFAULT as i32,
            toggle_key: DEFAULT_TOGGLE_KEY as i32,
            ui_scale: 1.0,
            menu_opacity: 0.9,
            freeze_input: true,
            release_cursor: true,
            show_fps: true,
            open_on_start: false,
            highlight: false,
        }
    }
}

impl Settings {
    /// Read every value from `.cleo_config.ini`, falling back to defaults.
    pub fn load() -> Self {
        let defaults = Self::default();

        Self {
            theme: cfg_get_int(K_THEME, defaults.theme).clamp(0, 2),
            toggle_key: cfg_get_int(K_TOGGLE_KEY, defaults.toggle_key),
            ui_scale: cfg_get_float(K_UI_SCALE, defaults.ui_scale).clamp(0.5, 2.0),
            menu_opacity: cfg_get_float(K_MENU_OPACITY, defaults.menu_opacity).clamp(0.15, 1.0),
            freeze_input: cfg_get_int(K_FREEZE_INPUT, defaults.freeze_input as i32) != 0,
            release_cursor: cfg_get_int(K_RELEASE_CURSOR, defaults.release_cursor as i32) != 0,
            show_fps: cfg_get_int(K_SHOW_FPS, defaults.show_fps as i32) != 0,
            open_on_start: cfg_get_int(K_OPEN_ON_START, defaults.open_on_start as i32) != 0,
            highlight: cfg_get_int(K_HIGHLIGHT, defaults.highlight as i32) != 0,
        }
    }

    /// Write every value back to `.cleo_config.ini`.
    pub fn save(&self) {
        cfg_set_int(K_THEME, self.theme);
        cfg_set_int(K_TOGGLE_KEY, self.toggle_key);
        cfg_set_float(K_UI_SCALE, self.ui_scale);
        cfg_set_float(K_MENU_OPACITY, self.menu_opacity);
        cfg_set_int(K_FREEZE_INPUT, self.freeze_input as i32);
        cfg_set_int(K_RELEASE_CURSOR, self.release_cursor as i32);
        cfg_set_int(K_SHOW_FPS, self.show_fps as i32);
        cfg_set_int(K_OPEN_ON_START, self.open_on_start as i32);
        cfg_set_int(K_HIGHLIGHT, self.highlight as i32);
    }

    pub fn toggle_key_label(&self) -> String {
        TOGGLE_KEYS
            .iter()
            .find(|k| k.vk as i32 == self.toggle_key)
            .map(|k| k.label.to_string())
            .unwrap_or_else(|| format!("0x{:02X}", self.toggle_key))
    }

    /// The bool behind a switch row, if the row is a switch.
    fn flag_mut(&mut self, setting: Setting) -> Option<&mut bool> {
        Some(match setting {
            Setting::FreezeInput => &mut self.freeze_input,
            Setting::ReleaseCursor => &mut self.release_cursor,
            Setting::ShowFps => &mut self.show_fps,
            Setting::OpenOnStart => &mut self.open_on_start,
            Setting::Highlight => &mut self.highlight,
            _ => return None,
        })
    }
}

// ------------------------------------------------------------ script tracing

/// CLEO's script-tracing levels, as the `DebugUtils` plugin's
/// `Plugins.DebugUtils.ScriptLog.Enabled` switch understands them.
pub const TRACING_LEVELS: [(&str, i32); 3] = [
    (strings::logs::TRACING_OFF, 0),
    (strings::logs::TRACING_ON_CRASH, 1),
    (strings::logs::TRACING_FULL, 2),
];

const K_TRACING: &str = "DebugUtils.ScriptLog.Enabled";

/// Current tracing level, clamped to the values the plugin knows.
pub fn tracing_level() -> i32 {
    cfg_get_int(K_TRACING, 0).clamp(0, 2)
}

/// Label of a tracing level, for the combo box.
pub fn tracing_label(level: i32) -> &'static str {
    TRACING_LEVELS
        .iter()
        .find(|(_, value)| *value == level)
        .map(|(label, _)| *label)
        .unwrap_or(strings::logs::TRACING_OFF)
}

// -------------------------------------------------------------- script debug

// The debug view's own switches. They are not rows on the Settings page: they belong to the
// debug window and are written here so a session keeps the layout it was left with.
const K_DEBUG_ENABLED: &str = "Dashboard.Debug.Enabled";
const K_DEBUG_NATIVE: &str = "Dashboard.Debug.AllowNativeControl";
const K_DEBUG_COMPACT: &str = "Dashboard.Debug.Compact";
const K_DEBUG_FIELDS: &str = "Dashboard.Debug.ShowFields";
const K_DEBUG_SPLIT: &str = "Dashboard.Debug.SplitPercent";
const K_DEBUG_X: &str = "Dashboard.Debug.WindowX";
const K_DEBUG_Y: &str = "Dashboard.Debug.WindowY";
const K_DEBUG_W: &str = "Dashboard.Debug.WindowWidth";
const K_DEBUG_H: &str = "Dashboard.Debug.WindowHeight";

/// Whether the opcode callbacks are registered at all. Read once, at startup: turning it
/// off takes effect on the next load of the overlay.
pub fn debug_enabled() -> bool {
    cfg_get_int(K_DEBUG_ENABLED, 1) != 0
}

/// Whether Pause/Step may park a mission script. Off by default: parking one of those
/// mid-mission leaves the mission state inconsistent.
pub fn allow_native_control() -> bool {
    cfg_get_int(K_DEBUG_NATIVE, 0) != 0
}

/// Where the debugger window was left, as `[x, y, width, height]`; `None` until it has been
/// moved or resized once. One geometry for every debugger window rather than one per script.
pub fn debug_window() -> Option<[f32; 4]> {
    let width = cfg_get_int(K_DEBUG_W, 0);
    let height = cfg_get_int(K_DEBUG_H, 0);
    if width < 240 || height < 200 {
        return None;
    }

    Some([
        cfg_get_int(K_DEBUG_X, 0) as f32,
        cfg_get_int(K_DEBUG_Y, 0) as f32,
        width as f32,
        height as f32,
    ])
}

pub fn set_debug_window(rect: [f32; 4]) {
    cfg_set_int(K_DEBUG_X, rect[0].round() as i32);
    cfg_set_int(K_DEBUG_Y, rect[1].round() as i32);
    cfg_set_int(K_DEBUG_W, rect[2].round() as i32);
    cfg_set_int(K_DEBUG_H, rect[3].round() as i32);
}

/// Whether the trace shows compact lines.
pub fn debug_compact() -> bool {
    cfg_get_int(K_DEBUG_COMPACT, 0) != 0
}

pub fn set_debug_compact(compact: bool) {
    cfg_set_int(K_DEBUG_COMPACT, compact as i32);
}

/// Where the debugger's split between the listing and the variables sits, as a fraction of
/// the pane width.
pub fn debug_split() -> f32 {
    let percent = cfg_get_int(K_DEBUG_SPLIT, 58).clamp(25, 80);
    percent as f32 / 100.0
}

pub fn set_debug_split(split: f32) {
    cfg_set_int(K_DEBUG_SPLIT, (split.clamp(0.25, 0.80) * 100.0).round() as i32);
}

/// Whether the debugger shows the fields/flags panel. Off by default: it is as tall as the
/// pane above it, and the pane above it is what the window is for.
pub fn debug_fields() -> bool {
    cfg_get_int(K_DEBUG_FIELDS, 0) != 0
}

pub fn set_debug_fields(show: bool) {
    cfg_set_int(K_DEBUG_FIELDS, show as i32);
}

/// Which log the Logs tab was showing when the menu last closed: an index into
/// [`logtail::LOG_SOURCES`].
pub fn log_source() -> usize {
    let last = logtail::LOG_SOURCES.len().saturating_sub(1) as i32;
    cfg_get_int(K_LOG_SOURCE, 0).clamp(0, last) as usize
}

/// Remember the selected log, so the next load opens the same one.
pub fn set_log_source(source: usize) {
    let last = logtail::LOG_SOURCES.len().saturating_sub(1);
    cfg_set_int(K_LOG_SOURCE, source.min(last) as i32);
}

/// Which optional columns the script table shows. The first two, the row number and the
/// path, are always there: a table without them is not a table.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ScriptColumn {
    Status,
    Mode,
    Size,
    Memory,
}

impl ScriptColumn {
    /// In table order.
    pub const ALL: [ScriptColumn; 4] = [
        ScriptColumn::Status,
        ScriptColumn::Size,
        ScriptColumn::Memory,
        ScriptColumn::Mode,
    ];

    /// The caption in the column picker, which is the table's own header.
    pub fn label(self) -> &'static str {
        match self {
            ScriptColumn::Status => crate::strings::scripts::HEADER_STATUS,
            ScriptColumn::Mode => crate::strings::scripts::HEADER_MODE,
            ScriptColumn::Size => crate::strings::scripts::HEADER_SIZE,
            ScriptColumn::Memory => crate::strings::scripts::HEADER_MEMORY,
        }
    }

    /// The config key suffix, so a renamed caption does not lose the setting.
    fn key(self) -> &'static str {
        match self {
            ScriptColumn::Status => "Dashboard.Scripts.ColumnStatus",
            ScriptColumn::Mode => "Dashboard.Scripts.ColumnMode",
            ScriptColumn::Size => "Dashboard.Scripts.ColumnSize",
            ScriptColumn::Memory => "Dashboard.Scripts.ColumnMemory",
        }
    }
}

/// Whether a column is shown. On unless the user turned it off in the column picker.
pub fn script_column(column: ScriptColumn) -> bool {
    cfg_get_int(column.key(), 1) != 0
}

pub fn set_script_column(column: ScriptColumn, shown: bool) {
    cfg_set_int(column.key(), shown as i32);
}

/// The rest of the DebugUtils script log's keys, in the plugin's own order.
///
/// The plugin reads them in `ScriptLog::LoadConfig`, which runs when logging starts, so a
/// new value lands the next time the level changes or the next save load.
const K_LOG_MAX_SIZE: &str = "DebugUtils.ScriptLog.MaxSize";
const K_LOG_CUSTOM_ONLY: &str = "DebugUtils.ScriptLog.OnlyCustomScripts";
const K_LOG_DEBUG_ONLY: &str = "DebugUtils.ScriptLog.OnlyDebugScripts";
const K_LOG_OFFSETS: &str = "DebugUtils.ScriptLog.Offsets";
const K_LOG_OPCODES: &str = "DebugUtils.ScriptLog.Opcodes";

/// What `OnlyCustomScripts` can be: the plugin's `-1` is its "decide from the game state".
pub const LOG_SCOPE_LEVELS: [(&str, i32); 3] = [
    (strings::logs::LOG_SCOPE_AUTO, -1),
    (strings::logs::LOG_SCOPE_ALL, 0),
    (strings::logs::LOG_SCOPE_CUSTOM, 1),
];

pub fn log_scope_label(scope: i32) -> &'static str {
    LOG_SCOPE_LEVELS
        .iter()
        .find(|(_, value)| *value == scope)
        .map(|(label, _)| *label)
        .unwrap_or(strings::logs::LOG_SCOPE_AUTO)
}

/// Log file size cap in megabytes, `0` for no cap.
pub fn log_max_size() -> i32 {
    cfg_get_int(K_LOG_MAX_SIZE, 250).clamp(0, 4096)
}

pub fn set_log_max_size(megabytes: i32) {
    cfg_set_int(K_LOG_MAX_SIZE, megabytes.clamp(0, 4096));
}

pub fn log_custom_only() -> i32 {
    cfg_get_int(K_LOG_CUSTOM_ONLY, -1).clamp(-1, 1)
}

pub fn set_log_custom_only(scope: i32) {
    cfg_set_int(K_LOG_CUSTOM_ONLY, scope.clamp(-1, 1));
}

pub fn log_debug_only() -> bool {
    cfg_get_int(K_LOG_DEBUG_ONLY, 0) != 0
}

pub fn set_log_debug_only(only: bool) {
    cfg_set_int(K_LOG_DEBUG_ONLY, only as i32);
}

pub fn log_offsets() -> bool {
    cfg_get_int(K_LOG_OFFSETS, 1) != 0
}

pub fn set_log_offsets(on: bool) {
    cfg_set_int(K_LOG_OFFSETS, on as i32);
}

pub fn log_opcodes() -> bool {
    cfg_get_int(K_LOG_OPCODES, 0) != 0
}

pub fn set_log_opcodes(on: bool) {
    cfg_set_int(K_LOG_OPCODES, on as i32);
}

/// Persist a new tracing level.
///
/// The write goes through `CLEO_SetConfigInt`, which fires the `ConfigUpdate` callbacks; the
/// DebugUtils plugin's script log listens for it, re-reads the level and switches its hooks at
/// the end of the frame, so a new level applies while the game is running.
pub fn set_tracing_level(level: i32) {
    cfg_set_int(K_TRACING, level.clamp(0, 2));
}

/// Slider value being dragged, committed once the user releases the slider.
static PENDING_SCALE: Mutex<Option<f32>> = Mutex::new(None);

/// Set while the opacity slider is being dragged, so the commit on release only happens for
/// a real edit.
static OPACITY_DIRTY: AtomicBool = AtomicBool::new(false);

static SETTINGS: OnceLock<Mutex<Settings>> = OnceLock::new();

fn store() -> &'static Mutex<Settings> {
    SETTINGS.get_or_init(|| Mutex::new(Settings::load()))
}

/// Current settings snapshot.
pub fn get() -> Settings {
    *store().lock().unwrap()
}

/// Update one setting in memory only, for live previews while a slider is dragged.
/// The ini is written once, when the caller commits with [`set`].
pub fn preview(settings: Settings) {
    *store().lock().unwrap() = settings;
}

/// Replace the settings, persist them and push the ones the hooks care about.
pub fn set(settings: Settings) {
    *store().lock().unwrap() = settings;
    settings.save();
    apply(settings);
}

/// Mirror settings that the overlay hooks read from other threads.
pub fn apply(settings: Settings) {
    overlay::RELEASE_CURSOR.store(settings.release_cursor, std::sync::atomic::Ordering::Relaxed);
    overlay::FREEZE_INPUT.store(settings.freeze_input, std::sync::atomic::Ordering::Relaxed);
}


// ---------------------------------------------------------------- descriptors

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    Theme,
    ToggleKey,
    UiScale,
    MenuOpacity,
    FreezeInput,
    ReleaseCursor,
    ShowFps,
    OpenOnStart,
    Highlight,
}

/// The caption a group of rows is drawn under.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Appearance,
    Behaviour,
}

impl Section {
    pub fn label(self) -> &'static str {
        match self {
            Section::Appearance => strings::settings::SECTION_APPEARANCE,
            Section::Behaviour => strings::settings::SECTION_BEHAVIOUR,
        }
    }
}

pub struct SettingDef {
    pub setting: Setting,
    pub section: Section,
    pub label: &'static str,
    pub tooltip: &'static str,
}

const fn def(
    setting: Setting,
    section: Section,
    label: &'static str,
    tooltip: &'static str,
) -> SettingDef {
    SettingDef {
        setting,
        section,
        label,
        tooltip,
    }
}

/// The rows in on-screen order. A section caption is drawn wherever `section` changes.
pub static SCHEMA: &[SettingDef] = &[
    def(
        Setting::Theme,
        Section::Appearance,
        strings::settings::THEME,
        strings::settings::TIP_THEME,
    ),
    def(
        Setting::ToggleKey,
        Section::Appearance,
        strings::settings::TOGGLE_KEY,
        strings::settings::TIP_TOGGLE_KEY,
    ),
    def(
        Setting::UiScale,
        Section::Appearance,
        strings::settings::UI_SCALE,
        strings::settings::TIP_UI_SCALE,
    ),
    def(
        Setting::MenuOpacity,
        Section::Appearance,
        strings::settings::MENU_OPACITY,
        strings::settings::TIP_MENU_OPACITY,
    ),
    def(
        Setting::FreezeInput,
        Section::Behaviour,
        strings::settings::FREEZE_INPUT,
        strings::settings::TIP_FREEZE_INPUT,
    ),
    def(
        Setting::ReleaseCursor,
        Section::Behaviour,
        strings::settings::RELEASE_CURSOR,
        strings::settings::TIP_RELEASE_CURSOR,
    ),
    def(
        Setting::ShowFps,
        Section::Behaviour,
        strings::settings::SHOW_FPS,
        strings::settings::TIP_SHOW_FPS,
    ),
    def(
        Setting::OpenOnStart,
        Section::Behaviour,
        strings::settings::OPEN_ON_START,
        strings::settings::TIP_OPEN_ON_START,
    ),
    def(
        Setting::Highlight,
        Section::Behaviour,
        strings::highlight::LABEL,
        strings::highlight::TIP,
    ),
];

/// The sections in draw order, with the number of rows in each.
pub fn sections() -> Vec<(Section, usize)> {
    let mut sections: Vec<(Section, usize)> = Vec::new();
    for def in SCHEMA {
        match sections.last_mut() {
            Some((section, rows)) if *section == def.section => *rows += 1,
            _ => sections.push((def.section, 1)),
        }
    }
    sections
}

/// Render one descriptor against `settings`. Returns whether the value changed.
pub fn row(ui: &Ui, settings: &mut Settings, def: &SettingDef) -> bool {
    labeled(ui, def.label);

    match def.setting {
        Setting::Theme => {
            let current = theme::Theme::from_int(settings.theme);
            let mut changed = false;
            theme::dropdown(
                ui,
                &format!("setting_{}", def.label),
                current.label(),
                def.tooltip,
                || {
                    for (index, label) in theme::Theme::ALL.iter().enumerate() {
                        let selected = settings.theme == index as i32;
                        if ui.selectable_config(*label).selected(selected).build() {
                            settings.theme = index as i32;
                            changed = true;
                        }
                    }
                },
            );
            changed
        }
        Setting::ToggleKey => {
            let mut changed = false;
            theme::dropdown(
                ui,
                &format!("setting_{}", def.label),
                &settings.toggle_key_label(),
                def.tooltip,
                || {
                    for key in TOGGLE_KEYS {
                        let selected = settings.toggle_key == key.vk as i32;
                        if ui
                            .selectable_config(key.label)
                            .selected(selected)
                            .build()
                        {
                            settings.toggle_key = key.vk as i32;
                            changed = true;
                        }
                    }
                },
            );
            changed
        }
        Setting::UiScale => {
            ui.set_next_item_width(210.0);

            let mut value = settings.ui_scale;
            ui.slider_config(format!("##{}", def.label), 0.5, 2.0)
                .display_format("%.2fx")
                .build(&mut value);

            let dragging = ui.is_item_active();
            let released = ui.is_item_deactivated_after_edit();
            theme::hint(ui, def.tooltip);

            // Rescaling the font atlas every frame while the slider is dragged makes the
            // whole menu jitter, so the scale only commits on release.
            if dragging {
                if let Ok(mut pending) = PENDING_SCALE.lock() {
                    *pending = Some(value);
                }
                false
            } else if released {
                let committed = PENDING_SCALE
                    .lock()
                    .ok()
                    .and_then(|mut pending| pending.take())
                    .unwrap_or(value);

                settings.ui_scale = (committed * 100.0).round() / 100.0;
                true
            } else {
                false
            }
        }
        Setting::MenuOpacity => {
            ui.set_next_item_width(210.0);

            let mut value = settings.menu_opacity;
            let edited = ui
                .slider_config(format!("##{}", def.label), 0.15, 1.0)
                .display_format("%.2f")
                .build(&mut value);

            // Read the item state before `hint`: a tooltip is a window of its own and would
            // leave "last item" pointing at its text instead of the slider.
            let released = ui.is_item_deactivated_after_edit();
            theme::hint(ui, def.tooltip);

            if edited {
                settings.menu_opacity = (value * 100.0).round() / 100.0;
                preview(*settings);
                OPACITY_DIRTY.store(true, Ordering::Relaxed);
            }

            released && OPACITY_DIRTY.swap(false, Ordering::Relaxed)
        }
        setting => {
            let Some(value) = settings.flag_mut(setting) else {
                return false;
            };
            let changed = theme::switch(ui, format!("##{}", def.label), value);
            theme::hint(ui, def.tooltip);
            changed
        }
    }
}

/// Width of the label column: the widest label plus a gap. Measured at run time so it
/// follows the font and the UI scale.
fn label_column(ui: &Ui) -> f32 {
    SCHEMA
        .iter()
        .map(|def| ui.calc_text_size(def.label)[0])
        .fold(0.0_f32, f32::max)
        + 24.0
}

/// Print a label in the label column and move the cursor to the control column, so every
/// control on the page starts at the same x.
fn labeled(ui: &Ui, label: &str) {
    let start = ui.cursor_pos()[0];
    ui.align_text_to_frame_padding();
    ui.text_colored(theme::Role::Dim.rgba(1.0), label);
    ui.same_line_with_pos(start + label_column(ui));
}

// ------------------------------------------------------------- config helpers

fn cs(value: &str) -> CString {
    CString::new(value.replace('\0', "")).unwrap_or_default()
}

fn cfg_get_int(key: &str, default: i32) -> i32 {
    let key = cs(key);
    unsafe { CLEO_GetConfigInt(key.as_ptr(), default) }
}

fn cfg_set_int(key: &str, value: i32) {
    let key = cs(key);
    unsafe {
        CLEO_SetConfigInt(key.as_ptr(), value);
    }
}

fn cfg_get_float(key: &str, default: f32) -> f32 {
    let key = cs(key);
    unsafe { CLEO_GetConfigFloat(key.as_ptr(), default) }
}

fn cfg_set_float(key: &str, value: f32) {
    let key = cs(key);
    unsafe {
        CLEO_SetConfigFloat(key.as_ptr(), value);
    }
}

/// Whether log and trace lines are coloured by what their parts are.
pub fn highlight() -> bool {
    get().highlight
}

