//! Overlay settings and the descriptor table that renders them.
//!
//! Values live in `.cleo_config.ini` (`[Plugins]` section) through the CLEO config API,
//! so they survive restarts and can be edited by hand:
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
/// Which log the Logs tab shows. Not in [`SCHEMA`]: it is UI state, remembered without a
/// row on the Settings page.
const K_LOG_SOURCE: &str = "Dashboard.General.LogSource";

pub const DEFAULT_TOGGLE_KEY: u8 = 0x2D; // Insert

/// One entry of the hotkey dropdown; the value is a virtual-key code.
pub struct ToggleKey {
    pub label: &'static str,
    pub vk: u8,
}

/// Every key the US layout has that is safe to grab: the alphabet, the digits, the
/// punctuation row, F1-F12, CapsLock and the navigation cluster. `Esc` is left out on
/// purpose - it is the one key the game always needs (pause menu, aborting cutscenes).
/// Arrows and the numpad are skipped too; they belong to the game.
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
    ToggleKey { label: "` (grave)", vk: 0xC0 },
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
    }

    pub fn toggle_key_label(&self) -> String {
        TOGGLE_KEYS
            .iter()
            .find(|k| k.vk as i32 == self.toggle_key)
            .map(|k| k.label.to_string())
            .unwrap_or_else(|| format!("0x{:02X}", self.toggle_key))
    }
}

// ------------------------------------------------------------ script tracing

/// CLEO's script-tracing levels as the `DebugUtils` plugin's
/// `Plugins.DebugUtils.ScriptLog.Enabled` switch understands them.
pub const TRACING_LEVELS: [(&str, i32); 3] = [(strings::logs::TRACING_OFF, 0), (strings::logs::TRACING_ON_CRASH, 1), (strings::logs::TRACING_FULL, 2)];

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

/// Persist a new tracing level.
///
/// The write goes through `CLEO_SetConfigInt`, which fires CLEO's `ConfigUpdate`
/// callback; the DebugUtils plugin listens and applies the change to the running session.
pub fn set_tracing_level(level: i32) {
    cfg_set_int(K_TRACING, level.clamp(0, 2));
}

/// Slider value being dragged; committed once the user releases the slider.
static PENDING_SCALE: Mutex<Option<f32>> = Mutex::new(None);

/// Set while the opacity slider is being dragged, so the commit on release only happens
/// for a real edit and not for a stray item-deactivated frame.
static OPACITY_DIRTY: AtomicBool = AtomicBool::new(false);

fn pending_scale() -> &'static Mutex<Option<f32>> {
    &PENDING_SCALE
}

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
}

pub struct SettingDef {
    pub setting: Setting,
    pub label: &'static str,
    pub tooltip: &'static str,
}

/// Order here is the order on screen.
pub static SCHEMA: &[SettingDef] = &[
    SettingDef {
        setting: Setting::Theme,
        label: strings::settings::THEME,
        tooltip: strings::settings::TIP_THEME,
    },
    SettingDef {
        setting: Setting::ToggleKey,
        label: strings::settings::TOGGLE_KEY,
        tooltip: strings::settings::TIP_TOGGLE_KEY,
    },
    SettingDef {
        setting: Setting::UiScale,
        label: strings::settings::UI_SCALE,
        tooltip: strings::settings::TIP_UI_SCALE,
    },
    SettingDef {
        setting: Setting::MenuOpacity,
        label: strings::settings::MENU_OPACITY,
        tooltip: strings::settings::TIP_MENU_OPACITY,
    },
    SettingDef {
        setting: Setting::FreezeInput,
        label: strings::settings::FREEZE_INPUT,
        tooltip: strings::settings::TIP_FREEZE_INPUT,
    },
    SettingDef {
        setting: Setting::ReleaseCursor,
        label: strings::settings::RELEASE_CURSOR,
        tooltip: strings::settings::TIP_RELEASE_CURSOR,
    },
    SettingDef {
        setting: Setting::ShowFps,
        label: strings::settings::SHOW_FPS,
        tooltip: strings::settings::TIP_SHOW_FPS,
    },
    SettingDef {
        setting: Setting::OpenOnStart,
        label: strings::settings::OPEN_ON_START,
        tooltip: strings::settings::TIP_OPEN_ON_START,
    },
];

/// Render one descriptor against `settings`. Returns whether the value changed.
pub fn row(ui: &Ui, settings: &mut Settings, def: &SettingDef) -> bool {
    match def.setting {
        Setting::Theme => {
            labeled_text(ui, def.label);
            ui.set_next_item_width(210.0);

            let current = theme::Theme::from_int(settings.theme);
            let mut changed = false;
            if let Some(_token) = ui.begin_combo(format!("##{}", def.label), current.label()) {
                for (index, label) in theme::Theme::ALL.iter().enumerate() {
                    let selected = settings.theme == index as i32;
                    if ui.selectable_config(*label).selected(selected).build() {
                        settings.theme = index as i32;
                        changed = true;
                    }
                }
            }
            hint(ui, def.tooltip);
            changed
        }
        Setting::ToggleKey => {
            labeled_text(ui, def.label);
            let mut changed = false;
            ui.set_next_item_width(210.0);
            // A taller popup than the default eight rows: the list holds the whole US
            // layout, and scrolling eight entries at a time was the slowest part of
            // picking a key.
            if let Some(_token) = ui.begin_combo_with_flags(
                format!("##{}", def.label),
                settings.toggle_key_label(),
                imgui::ComboBoxFlags::HEIGHT_LARGE,
            ) {
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
            }
            hint(ui, def.tooltip);
            changed
        }
        Setting::UiScale => {
            labeled_text(ui, def.label);
            ui.set_next_item_width(210.0);

            let mut value = settings.ui_scale;
            ui.slider_config(format!("##{}", def.label), 0.5, 2.0)
                .display_format("%.2fx")
                .build(&mut value);

            let dragging = ui.is_item_active();
            let released = ui.is_item_deactivated_after_edit();
            hint(ui, def.tooltip);

            // Rescaling the font atlas every frame while the slider is dragged makes the
            // whole menu jitter, so keep the committed scale until the user lets go.
            if dragging {
                if let Ok(mut pending) = pending_scale().lock() {
                    *pending = Some(value);
                }
                false
            } else if released {
                let committed = pending_scale()
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
            labeled_text(ui, def.label);
            ui.set_next_item_width(210.0);

            let mut value = settings.menu_opacity;
            let edited = ui
                .slider_config(format!("##{}", def.label), 0.15, 1.0)
                .display_format("%.2f")
                .build(&mut value);

            // Read the item state before `hint`: a tooltip is a window of its own and
            // would leave "last item" pointing at its text instead of the slider.
            let released = ui.is_item_deactivated_after_edit();
            hint(ui, def.tooltip);

            // Alpha costs nothing to change, so preview it live and persist on release -
            // the same write-on-release rule the UI scale slider follows, except that the
            // visual update is not deferred.
            if edited {
                settings.menu_opacity = (value * 100.0).round() / 100.0;
                preview(*settings);
                OPACITY_DIRTY.store(true, Ordering::Relaxed);
            }

            // Only consume the flag on the release frame: the edit happens on the frame
            // the mouse goes down, the release a few frames later.
            let dirty = released && OPACITY_DIRTY.swap(false, Ordering::Relaxed);
            dirty
        }
        Setting::FreezeInput => {
            labeled_checkbox(ui, def.label, def.tooltip, &mut settings.freeze_input)
        }
        Setting::ReleaseCursor => {
            labeled_checkbox(ui, def.label, def.tooltip, &mut settings.release_cursor)
        }
        Setting::ShowFps => labeled_checkbox(ui, def.label, def.tooltip, &mut settings.show_fps),
        Setting::OpenOnStart => {
            labeled_checkbox(ui, def.label, def.tooltip, &mut settings.open_on_start)
        }
    }
}

/// Labels that are not settings but share the label column (the Storage rows).
pub const EXTRA_LABELS: [&str; 1] = [strings::settings::LABEL_SETTINGS_FILE];

/// Width of the label column in pixels: the widest label in the tab plus a gap.
/// Measured at runtime so it follows the font and the UI scale; `{:<24}` did not,
/// because a proportional font has no fixed character width.
fn label_column(ui: &Ui) -> f32 {
    let widest = SCHEMA
        .iter()
        .map(|def| def.label)
        .chain(EXTRA_LABELS)
        .map(|label| ui.calc_text_size(label)[0])
        .fold(0.0_f32, f32::max);

    widest + 24.0
}

/// Print a label in the label column and move the cursor to the control column, so every
/// control in the tab starts at the same x.
pub fn labeled(ui: &Ui, label: &str) {
    let column = label_column(ui);
    let start = ui.cursor_pos()[0];

    ui.align_text_to_frame_padding();
    ui.text(label);
    ui.same_line_with_pos(start + column);
}

fn labeled_text(ui: &Ui, label: &str) {
    labeled(ui, label);
}

fn labeled_checkbox(ui: &Ui, label: &str, tooltip: &str, value: &mut bool) -> bool {
    labeled(ui, label);
    let changed = theme::switch(ui, format!("##{}", label), value);
    hint(ui, tooltip);
    changed
}

fn hint(ui: &Ui, tooltip: &str) {
    theme::hint(ui, tooltip);
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

#[allow(dead_code)]
fn cfg_get_text(key: &str, default: &str) -> String {
    let key_c = cs(key);
    let default_c = cs(default);
    let mut buffer = [0i8; 512];
    unsafe {
        CLEO_GetConfigText(
            key_c.as_ptr(),
            default_c.as_ptr(),
            buffer.as_mut_ptr(),
            buffer.len() as u32,
        );
    }

    unsafe { std::ffi::CStr::from_ptr(buffer.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

#[allow(dead_code)]
fn cfg_set_text(key: &str, value: &str) {
    let key_c = cs(key);
    let value_c = cs(value);
    unsafe {
        CLEO_SetConfigText(key_c.as_ptr(), value_c.as_ptr());
    }
}
