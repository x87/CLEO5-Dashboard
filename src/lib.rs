#[macro_use]
extern crate static_assertions;

mod config;
mod error;
mod github;
mod logtail;
mod overlay;
mod plugins;
mod settings;
mod sdk;
mod strings;
mod table;
mod theme;
mod version;

use ctor::*;
use crate::strings::{pretty_size, pct};
use std::collections::HashMap;
use hudhook::{
    windows::Win32::{
        Foundation::HMODULE, System::LibraryLoader::GetModuleFileNameA,
        System::SystemInformation::GetTickCount,
        UI::Input::KeyboardAndMouse::GetAsyncKeyState,
    },
    *,
};
use imgui::{Condition, Context, Ui};
use sdk::*;

fn ptr_to_rust_str<'a>(ptr: *const i8) -> &'a str {
    unsafe {
        assert!(!ptr.is_null());
        std::ffi::CStr::from_ptr(ptr).to_str().unwrap()
    }
}

#[derive(Clone, Copy)]
enum Colors {
    Success = 0x50C878,
    Warning = 0xB87333,
    Error = 0xAD343E,
    Secondary = 0xAAAAAA,
    Text = 0xFFFFFFF,
}

/// Resolve a palette role to the colour the current theme draws it with.
fn color_of(value: Colors) -> imgui::ImColor32 {
    match value {
        Colors::Success => theme::success_color(),
        Colors::Warning => theme::warning_color(),
        Colors::Error => theme::error_color(),
        Colors::Secondary => theme::dim_color(),
        Colors::Text => theme::text_color(),
    }
}

/// The compat modes the Compat column can pin a script to, newest first. The SDK has
/// finer steps (4.0 / 4.2 / 4.3 / 4.4); they all behave as "CLEO 4" here.
static COMPAT_LEVELS: [(u32, &str); 3] = [
    (CLEO_VER_5, strings::scripts::COMPAT_5),
    (CLEO_VER_4, strings::scripts::COMPAT_4),
    (CLEO_VER_3, strings::scripts::COMPAT_3),
];

/// The entry a script's version belongs to.
fn compat_bucket(version: u32) -> u32 {
    if version >= CLEO_VER_5 {
        CLEO_VER_5
    } else if version >= CLEO_VER_4 {
        CLEO_VER_4
    } else {
        CLEO_VER_3
    }
}

fn compat_label(version: u32) -> &'static str {
    let bucket = compat_bucket(version);
    COMPAT_LEVELS
        .iter()
        .find(|(value, _)| *value == bucket)
        .map(|(_, label)| *label)
        .unwrap_or(strings::scripts::COMPAT_UNKNOWN)
}

static mut UPDATE_STATUS: u32 = 0;

/// Menu visibility lives in `overlay` so the IAT detours and the message filter can read
/// it without touching the render loop.
fn is_open() -> bool {
    overlay::is_open()
}

/// Shorten a script path to `budget` pixels, keeping its head and its tail.
///
/// Script paths repeat their middle (`modloader\<mod>\cleo\<file>`), so dropping it keeps
/// both the mod that owns the file and the file name, which is what tells two scripts
/// apart. As many trailing folders as fit are kept.
fn elide_path(ui: &Ui, path: &str, budget: f32) -> String {
    use std::path::MAIN_SEPARATOR as SEP;

    let fits = |text: &str| ui.calc_text_size(text)[0] <= budget;
    if fits(path) {
        return path.to_string();
    }

    let parts: Vec<&str> = path.split(SEP).collect();
    if parts.len() < 2 {
        return elide_tail(ui, path, budget);
    }

    let head = format!("{}{}...{}", parts[0], SEP, SEP);
    let mut tail = String::new();
    for part in parts[1..].iter().rev() {
        let candidate = if tail.is_empty() {
            (*part).to_string()
        } else {
            format!("{part}{SEP}{tail}")
        };
        // Always keep the file name; if even that overflows, cut its head.
        if tail.is_empty() || fits(&format!("{head}{candidate}")) {
            tail = candidate;
        } else {
            break;
        }
    }

    let shortened = format!("{head}{tail}");
    if fits(&shortened) {
        shortened
    } else {
        elide_tail(ui, &shortened, budget)
    }
}

/// Drop characters from the front, keeping a leading `...`.
fn elide_tail(ui: &Ui, text: &str, budget: f32) -> String {
    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() {
        chars.remove(0);
        let candidate: String = format!("...{}", chars.iter().collect::<String>());
        if ui.calc_text_size(&candidate)[0] <= budget {
            return candidate;
        }
    }
    String::new()
}

/// Minimum time between two accepted hotkey presses.
///
/// A single tap can show up as two down-edges (key repeat, a remapper, or the OS applying
/// a lock key's state over two frames), which used to open the menu and close it again in
/// the same instant.
const TOGGLE_COOLDOWN_MS: u32 = 250;

/// Rising-edge detector for the menu hotkey, read straight from the keyboard state so it
/// works while the game or the menu is busy, independently of ImGui input.
///
/// CapsLock needs no special case: `GetAsyncKeyState` reports "is down" in bit 15 for it
/// as well. Its lock bit is deliberately not used - that bit only means "pressed since the
/// previous call", so watching it made one tap look like a press and then a release, and
/// the menu flashed open and closed.
struct ToggleKeyState {
    /// The key this tracker belongs to, so switching the hotkey mid-session cannot
    /// report the old key's state as a fresh press.
    key: i32,
    was_down: bool,
    /// Last accepted press, for [`TOGGLE_COOLDOWN_MS`].
    last_press_ms: u32,
}

impl ToggleKeyState {
    fn new() -> Self {
        Self {
            key: -1,
            was_down: false,
            last_press_ms: 0,
        }
    }

    /// True on the frame the hotkey goes from up to down.
    fn pressed(&mut self, key: i32) -> bool {
        let down = async_key_down(key);

        if key != self.key {
            self.key = key;
            self.was_down = down;
            return false;
        }

        let edge = down && !self.was_down;
        self.was_down = down;
        if !edge {
            return false;
        }

        let now = unsafe { GetTickCount() };
        if now.wrapping_sub(self.last_press_ms) < TOGGLE_COOLDOWN_MS {
            return false;
        }
        self.last_press_ms = now;
        true
    }
}

fn async_key_down(key: i32) -> bool {
    (unsafe { GetAsyncKeyState(key) }) as u16 & 0x8000 != 0
}

struct Dashboard {
    toggle: ToggleKeyState,
    /// Theme the ImGui style was last built for.
    theme_applied: i32,
    /// Opacity the ImGui style was last built for.
    opacity_applied: f32,
    /// Visible tab. Kept here because the strip is drawn by hand.
    tab: usize,
    /// `cleo_script.log`, written by the DebugUtils plugin.
    script_log: logtail::LogTail,
    /// CLEO's own `cleo.log`.
    cleo_log: logtail::LogTail,
    /// Which log the Logs tab shows, as an index into [`logtail::LOG_SOURCES`]. Remembered
    /// in `.cleo_config.ini`, so it survives a reload.
    log_source: usize,
    /// Text in the filter box. It has to outlive the frame: a buffer rebuilt every frame is
    /// always empty.
    filter: String,
    /// Scripts whose compatibility mode was changed while they run: absolute path to the
    /// mode the script was *loaded* with. The Compat column says "Reload required" while
    /// the selected mode differs from that one, so switching away and back cancels it.
    reload_pending: HashMap<String, u32>,
}

impl Default for Dashboard {
    fn default() -> Self {
        Self {
            toggle: ToggleKeyState::new(),
            theme_applied: -1,
            opacity_applied: -1.0,
            tab: 0,
            script_log: logtail::LogTail::new("cleo_script.log"),
            cleo_log: logtail::LogTail::plain("cleo.log"),
            log_source: settings::log_source(),
            filter: String::new(),
            reload_pending: HashMap::new(),
        }
    }
}

/// CLEO's directory, where `.cleo_config.ini` and the logs live.
fn cleo_dir() -> String {
    let cleo_dir = unsafe { CLEO_GetCleoDirectory() };
    if cleo_dir.is_null() {
        ".".to_string()
    } else {
        ptr_to_rust_str(cleo_dir).to_string()
    }
}

/// The `[Plugins]` section of CLEO's config, which sits next to CLEO.asi.
fn settings_file_path() -> std::path::PathBuf {
    std::path::Path::new(&cleo_dir()).join(".cleo_config.ini")
}

fn make_path_relative_to_exe(path: &str) -> String {
    if let Ok(current_exe_path) = std::env::current_exe() {
        if let (Ok(canonical_exe_dir), Ok(canonical_stream_path)) = (
            current_exe_path.parent().unwrap().canonicalize(),
            std::path::Path::new(&path).canonicalize(),
        ) {
            if let Ok(relative_path) = canonical_stream_path.strip_prefix(&canonical_exe_dir) {
                return relative_path.to_str().unwrap_or(path).to_string();
            }
        }
    }

    return path.to_string();
}

impl ImguiRenderLoop for Dashboard {
    fn initialize(&mut self, ctx: &mut Context, _render_context: &mut dyn RenderContext) {
        log::info!("Imgui initialized");
        theme::register_fonts(ctx);

        let theme = settings::get().theme;
        theme::set_current(theme::Theme::from_int(theme));
        theme::apply(ctx);
        self.theme_applied = theme;

        // No ImGui layout file: the window is pinned and sized from the content, so there
        // is no layout worth persisting.
        ctx.set_ini_filename(None);

        let current = settings::get();
        settings::apply(current);
        ctx.io_mut().font_global_scale = current.ui_scale;

        if !overlay::install_iat_hooks() {
            log::warn!("could not redirect USER32 ClipCursor/SetCursorPos imports");
        }

        if !overlay::install_input_freeze_hook() {
            log::warn!("could not detour the mouse poll; Freeze game input will not stop mouse look");
        }

        if current.open_on_start {
            overlay::set_open(true);
        }
    }

    fn before_render<'a>(
        &'a mut self,
        ctx: &mut Context,
        _render_context: &'a mut dyn RenderContext,
    ) {
        let current = settings::get();
        if self.toggle.pressed(current.toggle_key) {
            overlay::set_open(!is_open());
        }

        // Theme switches and the opacity slider land in the same style, so either change
        // re-applies it.
        theme::set_opacity(current.menu_opacity);
        if current.theme != self.theme_applied || current.menu_opacity != self.opacity_applied {
            theme::set_current(theme::Theme::from_int(current.theme));
            theme::apply(ctx);
            self.theme_applied = current.theme;
            self.opacity_applied = current.menu_opacity;
        }

        let io = ctx.io_mut();
        io.mouse_draw_cursor = is_open();
        if io.font_global_scale != current.ui_scale {
            io.font_global_scale = current.ui_scale;
        }

        plugins::detect();
        // Only the log on screen is re-read: the other one costs a file read every half
        // second for lines nobody is looking at.
        match self.log_source {
            0 => self.cleo_log.refresh(),
            _ => self.script_log.refresh(),
        }

        if is_open() && current.freeze_input {
            unsafe { overlay::zero_mouse_delta() };
        }
    }

    /// Stops the game window receiving input while the menu is open. ImGui still gets it:
    /// hudhook feeds the window procedure first.
    fn message_filter(&self, _io: &imgui::Io) -> MessageFilter {
        if is_open() && settings::get().freeze_input {
            MessageFilter::InputAll
        } else {
            MessageFilter::empty()
        }
    }

    fn render(&mut self, ui: &mut imgui::Ui) {
        if !is_open() {
            return;
        }

        let [width, height] = ui.io().display_size;

        let num_scripts = unsafe { CLEO_GetNumberOfCustomScripts() };

        // Tall enough for the settings rows, never taller than the screen. The UI scale
        // grows the fonts, so the body and the footer have to grow with them.
        let scale = settings::get().ui_scale.max(0.5);
        // Two rows (version + the hint line) plus the child's padding.
        let footer_height = 46.0 * scale + 22.0;

        // The children are sized from the menu's content region: sizing them to the full
        // window made them overflow, which pulled scrollbars onto the menu and cut the last
        // footer row.
        let pad = 8.0;
        let inner_width = (width - pad * 2.0).max(320.0);

        let mut labels: Vec<&str> = vec![strings::tabs::SCRIPTS];
        if plugins::memory().is_some() {
            labels.push(strings::tabs::LIBRARIES);
        }
        if plugins::audio().is_some() {
            labels.push(strings::tabs::AUDIO);
        }
        labels.push(strings::tabs::LOGS);
        labels.push(strings::tabs::SETTINGS);

        let available_height = (height - pad - 4.0).max(240.0);
        let max_body = (available_height - footer_height - pad * 2.0).max(240.0);

        // The scrolling tables and the settings page want all the room they can get; the
        // script table only needs as much as it has rows.
        let tab_name = labels.get(self.tab).copied().unwrap_or(strings::tabs::SCRIPTS);
        let body_height = match tab_name {
            strings::tabs::LOGS | strings::tabs::LIBRARIES | strings::tabs::AUDIO => max_body,
            strings::tabs::SETTINGS => (660.0 * scale).min(max_body),
            _ => {
                // A row is exactly one frame tall; the frozen header row that holds the
                // filter box is one more.
                let row = ui.frame_height();
                (((num_scripts as f32) + 1.5) * row + 40.0)
                    .max(200.0)
                    .min(max_body)
            }
        };
        let colored = |title: &str, v: Colors| {
            ui.text_colored(color_of(v).to_rgba_f32s(), title);
        };

        // The three list tabs. Each table gets the room its tab was sized for. The widths
        // come from the content measured here, so the first frame shows everything whole; a
        // column only clips after the user drags it narrower.
        let script_table = |ui: &Ui,
                            reload_pending: &mut HashMap<String, u32>,
                            filter: &mut String,
                            height: f32| {
            let style = ui.clone_style();
            let spacing = style.item_spacing[0];
            let frame_pad = style.frame_padding[0];
            let width = |text: &str| ui.calc_text_size(text)[0];

            // The absolute path identifies a script across a reload, where the thread
            // pointer changes; the pending-reload list is keyed on it.
            let abs_path = |i: usize| -> String {
                let thread = unsafe { CLEO_GetCustomScriptByIndex(i) };
                let mut path = [0i8; 260];
                unsafe {
                    CLEO_GetScriptFileFullPath(thread, path.as_mut_ptr(), path.len() as u32)
                };
                ptr_to_rust_str(path.as_ptr() as _).to_string()
            };

            // A script that stopped running takes its "reload required" note with it.
            reload_pending.retain(|path, _| (0..num_scripts).any(|i| &abs_path(i) == path));

            // The filter is read from last frame's text: the box is edited halfway through
            // the same frame in which the rows are picked.
            let needle = filter.trim().to_lowercase();
            let visible: Vec<usize> = (0..num_scripts)
                .filter(|i| {
                    needle.is_empty()
                        || make_path_relative_to_exe(&abs_path(*i))
                            .to_lowercase()
                            .contains(&needle)
                })
                .collect();

            // Widest memory cell: the configured limits are the largest numbers it can
            // print, and the size is compacted so the count dominates.
            let mut max_count = 0;
            let mut max_size = 0;
            if let Some(memory) = plugins::memory() {
                unsafe {
                    (memory.allocation_limits)(&mut max_count, &mut max_size);
                }
            }
            let memory_sample = format!("{max_count}/{max_count} \u{00B7} {}", pretty_size(max_size));

            let index_col = (width("000") + spacing * 2.0).max(28.0);
            // The combo needs its widest label plus the arrow; a "Reload required" note
            // can make the column wider.
            let compat_col = {
                let combo = width(strings::scripts::COMPAT_UNKNOWN) + frame_pad * 2.0 + ui.text_line_height();
                let note = if reload_pending.is_empty() {
                    0.0
                } else {
                    width(strings::scripts::RELOAD_REQUIRED)
                };
                combo.max(note) + spacing
            };
            // Wide enough for the header *and* the switch, which is the wider of the two.
            let switch_col = |header: &str| {
                width(header).max((ui.text_line_height() + 1.0) * 1.7)
                    + frame_pad * 2.0
                    + spacing * 2.0
            };
            let size_col = width("888.88 MB").max(width(strings::scripts::HEADER_SIZE)) + spacing * 2.0;
            let memory_col = width(&memory_sample).max(width(strings::scripts::HEADER_MEMORY)) + spacing * 2.0;
            let actions_col =
                width(strings::scripts::BUTTON_DUMP) + width(strings::scripts::BUTTON_RELOAD) + frame_pad * 4.0 + spacing * 2.0;

            let columns = [
                table::Column::fixed(strings::table::HEADER_INDEX, index_col),
                // The path takes every pixel the measured columns do not need; whatever is
                // still too long is elided from the middle.
                table::Column::stretch(strings::scripts::HEADER_SCRIPT),
                table::Column::fixed(strings::scripts::HEADER_SIZE, size_col),
                table::Column::fixed(strings::scripts::HEADER_COMPAT, compat_col),
                table::Column::fixed(strings::scripts::HEADER_DEBUG, switch_col(strings::scripts::HEADER_DEBUG)),
                table::Column::fixed(
                    strings::scripts::HEADER_SUSPENDED,
                    switch_col(strings::scripts::HEADER_SUSPENDED),
                ),
                table::Column::fixed(strings::scripts::HEADER_MEMORY, memory_col),
                table::Column::fixed(strings::scripts::HEADER_ACTIONS, actions_col).trailing(),
            ];

            // A note under the combo makes that row a line taller.
            let notes = visible
                .iter()
                .filter(|i| reload_pending.contains_key(&abs_path(**i)))
                .count() as f32;
            // Only as tall as its rows, so the rules end with the last one instead of
            // running down an empty panel; extra rows make it scroll.
            let table_height = (ui.frame_height() * (visible.len() as f32 + 1.0)
                + notes * (ui.text_line_height() + style.item_spacing[1])
                + 8.0)
                .min(height);

            table::table(ui, "scripts_table", &columns, table_height, |ui| {
                table::header_row(ui);

                ui.table_next_column();
                table::caption_centered(ui, strings::table::HEADER_INDEX);

                // The filter box rides in the caption row of the column it filters.
                ui.table_next_column();
                table::caption(ui, strings::scripts::HEADER_SCRIPT);
                if num_scripts > 0 {
                    ui.same_line();
                    ui.dummy([5.0, 0.0]);
                    ui.same_line();
                    ui.set_next_item_width(table::text_room(ui));
                    ui.input_text("##_script_filter", filter)
                        .hint(strings::scripts::FILTER_PLACEHOLDER)
                        .build();
                }

                ui.table_next_column();
                table::caption(ui, strings::scripts::HEADER_SIZE);
                ui.table_next_column();
                table::caption(ui, strings::scripts::HEADER_COMPAT);
                ui.table_next_column();
                table::caption(ui, strings::scripts::HEADER_DEBUG);
                ui.table_next_column();
                table::caption(ui, strings::scripts::HEADER_SUSPENDED);
                ui.table_next_column();
                table::caption(ui, strings::scripts::HEADER_MEMORY);
                ui.table_next_column();
                table::caption(ui, strings::scripts::HEADER_ACTIONS);

                for &i in &visible {
                    ui.table_next_row();

                    // One lookup per row: the tooltip, the reload note and the pending-reload
                    // key all go by this path.
                    let absolute = abs_path(i);
                    let has_note = reload_pending.contains_key(&absolute);
                    // A row carrying a "Reload required" note is a line taller, and the
                    // hover band has to cover the row it really is.
                    table::hovered_row(
                        ui,
                        ui.frame_height()
                            + if has_note {
                                ui.text_line_height() + style.item_spacing[1]
                            } else {
                                0.0
                            },
                    );

                    ui.table_next_column();
                    table::cell_centered(
                        ui,
                        &format!("##_idx{}", i),
                        &format!("{}", i + 1),
                        Colors::Secondary,
                    );

                    ui.table_next_column();
                    let relative = make_path_relative_to_exe(&absolute);
                    let shown = elide_path(ui, &relative, table::text_room(ui));
                    // The elision hides the middle of the path, never its end.
                    table::cell(ui, &format!("##_path{}", i), &shown, Colors::Success, &relative);

                    ui.table_next_column();
                    let thread = unsafe { CLEO_GetCustomScriptByIndex(i) };
                    let size = unsafe { CLEO_GetScriptSize(thread) };
                    table::cell(
                        ui,
                        &format!("##_size{}", i),
                        &pretty_size(size),
                        Colors::Secondary,
                        &strings::scripts::size_tooltip(size),
                    );

                    ui.table_next_column();
                    let thread = unsafe { CLEO_GetCustomScriptByIndex(i) };
                    let version = unsafe { CLEO_GetScriptVersion(thread) };

                    // Anything older than CLEO 3 has no mode to map onto: show it and leave
                    // it alone instead of silently upgrading it.
                    if version < CLEO_VER_3 {
                        table::cell(
                            ui,
                            &format!("##_compat{}", i),
                            strings::scripts::COMPAT_UNKNOWN,
                            Colors::Error,
                            "",
                        );
                    } else {
                        ui.set_next_item_width(table::text_room(ui));
                        if let Some(_token) =
                            ui.begin_combo(format!("##_compat{}", i), compat_label(version))
                        {
                            for (value, label) in COMPAT_LEVELS {
                                if ui
                                    .selectable_config(label)
                                    .selected(compat_bucket(version) == value)
                                    .build()
                                {
                                    // Remember the mode the script was loaded with, so that
                                    // switching away and back cancels the note.
                                    let key = absolute.clone();
                                    let loaded = *reload_pending.entry(key.clone()).or_insert(version);

                                    unsafe { CLEO_SetScriptVersion(thread, value) };

                                    if compat_bucket(value) == compat_bucket(loaded) {
                                        reload_pending.remove(&key);
                                    }
                                }
                            }
                        }
                        theme::hint(ui, strings::scripts::TIP_COMPAT);
                        if reload_pending.contains_key(&absolute) {
                            colored(strings::scripts::RELOAD_REQUIRED, Colors::Warning);
                        }
                    }

                    ui.table_next_column();
                    let thread = unsafe { CLEO_GetCustomScriptByIndex(i) };
                    let mut enabled = unsafe { CLEO_GetScriptDebugMode(thread) };
                    if theme::switch(ui, format!("##_debug{}", i), &mut enabled) {
                        unsafe { CLEO_SetScriptDebugMode(thread, enabled) };
                    }
                    theme::hint(ui, strings::scripts::TIP_DEBUG);

                    ui.table_next_column();
                    let thread = unsafe { CLEO_GetCustomScriptByIndex(i) };
                    // A parked script has its wake time pushed to infinity, so it is skipped
                    // every frame; clearing it lets the script run again next frame.
                    let mut parked = unsafe { (*thread).m_nWakeTime == 0xFFFFFFFF };
                    if theme::switch(ui, format!("##_suspended{}", i), &mut parked) {
                        unsafe { (*thread).m_nWakeTime = if parked { 0xFFFFFFFF } else { 0 } };
                    }
                    theme::hint(ui, strings::scripts::TIP_SUSPENDED);

                    ui.table_next_column();
                    let thread = unsafe { CLEO_GetCustomScriptByIndex(i) };
                    let mut count = 0;
                    let mut size = 0;
                    if let Some(memory) = plugins::memory() {
                        unsafe {
                            (memory.allocation_info)(thread, &mut count, &mut size);
                        }
                    }

                    if count == 0 {
                        table::cell(
                            ui,
                            &format!("##_mem{}", i),
                            strings::scripts::MEMORY_NONE,
                            Colors::Secondary,
                            strings::scripts::TIP_MEMORY_NONE,
                        );
                    } else {
                        let usage = pct(count, max_count).max(pct(size, max_size));
                        table::cell(
                            ui,
                            &format!("##_mem{}", i),
                            &strings::scripts::memory_cell(count, max_count, &pretty_size(size)),
                            if usage < 50.0 {
                                Colors::Text
                            } else if usage < 90.0 {
                                Colors::Warning
                            } else {
                                Colors::Error
                            },
                            &strings::scripts::memory_tooltip(count, max_count, size, max_size),
                        );
                    }

                    ui.table_next_column();
                    // Accent text on no background, like the other buttons in the menu.
                    if theme::flat_button(ui, format!("Dump##{}", i)) {
                        unsafe {
                            let thread = CLEO_GetCustomScriptByIndex(i);
                            let size = CLEO_GetScriptSize(thread) as usize;
                            if size != 0 {
                                let mut buf = vec![0u8; size];
                                let _ = std::ptr::copy_nonoverlapping(
                                    (*thread).m_nBaseIp as *const u8,
                                    buf.as_mut_ptr(),
                                    size,
                                );

                                let filename = ptr_to_rust_str(CLEO_GetScriptFilename(thread));
                                let _ = std::fs::write(&format!("{}.dump", filename), buf);
                            }
                        };
                    }
                    theme::hint(ui, strings::scripts::TIP_DUMP);
                    ui.same_line();

                    if theme::flat_button(ui, format!("Reload##{}", i)) {
                        unsafe {
                            let thread = CLEO_GetCustomScriptByIndex(i);
                            // The new instance inherits the parent's compatibility mode, so
                            // reloading is what actually applies a compat change.
                            CLEO_CreateCustomScript(thread, CLEO_GetScriptFilename(thread), 0);
                            CLEO_TerminateScript(thread);
                        }
                        reload_pending.remove(&absolute);
                    }
                    theme::hint(ui, strings::scripts::TIP_RELOAD);
                }
            });
        };

        let library_table = |ui: &Ui, height: f32| {
            let Some(memory) = plugins::memory() else {
                return;
            };
            let num_libs = unsafe { (memory.number_of_libraries)() };
            let style = ui.clone_style();
            let spacing = style.item_spacing[0];
            let width = |text: &str| ui.calc_text_size(text)[0];

            // Read the list once: the widths and the rows both come from it.
            let mut rows: Vec<(String, usize)> = Vec::with_capacity(num_libs);
            for i in 0..num_libs {
                let mut handle = HMODULE(std::ptr::null_mut());
                let mut count = 0;
                if unsafe { (memory.library_by_index)(i, &mut handle, &mut count) } {
                    let mut lpfilename = [0u8; 260];
                    unsafe {
                        GetModuleFileNameA(Some(handle), &mut lpfilename);
                    }
                    let absolute = ptr_to_rust_str(lpfilename.as_ptr() as _);
                    rows.push((make_path_relative_to_exe(absolute), count));
                }
            }

            let index_col = (width("000") + spacing * 2.0).max(28.0);
            let count_col = rows
                .iter()
                .fold(width(strings::libraries::HEADER_LOADS), |widest, (_, count)| {
                    widest.max(width(&count.to_string()))
                })
                + spacing * 2.0;

            let columns = [
                table::Column::fixed(strings::table::HEADER_INDEX, index_col),
                table::Column::stretch(strings::libraries::HEADER_LIBRARY),
                table::Column::fixed(strings::libraries::HEADER_LOADS, count_col).trailing(),
            ];

            // Only as tall as its rows; see the script table.
            let table_height = (ui.frame_height() * (rows.len() as f32 + 1.0) + 8.0).min(height);

            table::table(ui, "libraries_table", &columns, table_height, |ui| {
                table::header_row(ui);
                for caption in [
                    strings::table::HEADER_INDEX,
                    strings::libraries::HEADER_LIBRARY,
                    strings::libraries::HEADER_LOADS,
                ] {
                    ui.table_next_column();
                    if caption == strings::table::HEADER_INDEX {
                        table::caption_centered(ui, caption);
                    } else {
                        table::caption(ui, caption);
                    }
                }

                for (i, (path, count)) in rows.iter().enumerate() {
                    ui.table_next_row();
                    table::hovered_row(ui, ui.frame_height());

                    ui.table_next_column();
                    table::cell_centered(
                        ui,
                        &format!("##_libidx{}", i),
                        &format!("{}", i + 1),
                        Colors::Secondary,
                    );

                    ui.table_next_column();
                    let shown = elide_path(ui, path, table::text_room(ui));
                    table::cell(ui, &format!("##_lib{}", i), &shown, Colors::Success, path);

                    ui.table_next_column();
                    table::cell(
                        ui,
                        &format!("##_libcount{}", i),
                        &count.to_string(),
                        Colors::Text,
                        "",
                    );
                }
            });
        };

        let audio_table = |ui: &Ui, height: f32| {
            let Some(audio) = plugins::audio() else {
                return;
            };
            let num_streams = unsafe { (audio.number_of_streams)() };
            let style = ui.clone_style();
            let spacing = style.item_spacing[0];
            let width = |text: &str| ui.calc_text_size(text)[0];

            // Every cell is formatted once, up front: a column has to be sized from the
            // text it will actually show, or the first frame clips it.
            struct Stream {
                name: String,
                kind: &'static str,
                length: String,
                state: &'static str,
                is_3d: &'static str,
                looped: &'static str,
                volume: String,
                speed: String,
            }

            let mut rows = Vec::with_capacity(num_streams);
            for i in 0..num_streams {
                let mut filename = [0i8; 260];
                let name = if unsafe {
                    (audio.filename_by_index)(i, filename.as_mut_ptr(), filename.len())
                } {
                    make_path_relative_to_exe(ptr_to_rust_str(filename.as_ptr() as _))
                } else {
                    String::new()
                };

                rows.push(Stream {
                    name,
                    kind: match unsafe { (audio.type_by_index)(i) } {
                        0 => strings::audio::TYPE_NONE,
                        1 => strings::audio::TYPE_SFX,
                        2 => strings::audio::TYPE_MUSIC,
                        3 => strings::audio::TYPE_UI,
                        _ => strings::audio::TYPE_UNKNOWN,
                    },
                    length: format!("{:.2}", unsafe { (audio.length_by_index)(i) }),
                    state: match unsafe { (audio.state_by_index)(i) } {
                        -1 => strings::audio::STATE_STOPPED,
                        0 => strings::audio::STATE_INACTIVE,
                        1 => strings::audio::STATE_PLAYING,
                        2 => strings::audio::STATE_PAUSED,
                        _ => strings::audio::STATE_UNKNOWN,
                    },
                    is_3d: if unsafe { (audio.is_3d_by_index)(i) } {
                        strings::audio::YES
                    } else {
                        strings::audio::NO
                    },
                    looped: if unsafe { (audio.is_looped_by_index)(i) } {
                        strings::audio::YES
                    } else {
                        strings::audio::NO
                    },
                    volume: format!("{:.2}", unsafe { (audio.volume_by_index)(i) }),
                    speed: format!("{:.2}", unsafe { (audio.speed_by_index)(i) }),
                });
            }

            let index_col = (width("000") + spacing * 2.0).max(28.0);
            let widest = |caption: &str, cells: &mut dyn Iterator<Item = f32>| -> f32 {
                cells.fold(width(caption), |widest, value| widest.max(value)) + spacing * 2.0
            };
            let length_room = width(" ") + width(strings::audio::SECONDS);
            let type_col = widest(strings::audio::HEADER_TYPE, &mut rows.iter().map(|r| width(r.kind)));
            let length_col = widest(
                strings::audio::HEADER_LENGTH,
                &mut rows.iter().map(|r| width(&r.length) + length_room),
            );
            let state_col = widest(strings::audio::HEADER_STATE, &mut rows.iter().map(|r| width(r.state)));
            let three_d_col = widest(strings::audio::HEADER_3D, &mut rows.iter().map(|r| width(r.is_3d)));
            let loop_col = widest(strings::audio::HEADER_LOOP, &mut rows.iter().map(|r| width(r.looped)));
            let volume_col = widest(strings::audio::HEADER_VOLUME, &mut rows.iter().map(|r| width(&r.volume)));
            let speed_col = widest(strings::audio::HEADER_SPEED, &mut rows.iter().map(|r| width(&r.speed)));

            let columns = [
                table::Column::fixed(strings::table::HEADER_INDEX, index_col),
                table::Column::stretch(strings::audio::HEADER_FILENAME),
                table::Column::fixed(strings::audio::HEADER_TYPE, type_col),
                table::Column::fixed(strings::audio::HEADER_LENGTH, length_col),
                table::Column::fixed(strings::audio::HEADER_STATE, state_col),
                table::Column::fixed(strings::audio::HEADER_3D, three_d_col),
                table::Column::fixed(strings::audio::HEADER_LOOP, loop_col),
                table::Column::fixed(strings::audio::HEADER_VOLUME, volume_col),
                table::Column::fixed(strings::audio::HEADER_SPEED, speed_col).trailing(),
            ];

            // Only as tall as its rows; see the script table.
            let table_height = (ui.frame_height() * (rows.len() as f32 + 1.0) + 8.0).min(height);

            table::table(ui, "audio_table", &columns, table_height, |ui| {
                table::header_row(ui);
                for caption in [
                    strings::table::HEADER_INDEX,
                    strings::audio::HEADER_FILENAME,
                    strings::audio::HEADER_TYPE,
                    strings::audio::HEADER_LENGTH,
                    strings::audio::HEADER_STATE,
                    strings::audio::HEADER_3D,
                    strings::audio::HEADER_LOOP,
                    strings::audio::HEADER_VOLUME,
                    strings::audio::HEADER_SPEED,
                ] {
                    ui.table_next_column();
                    if caption == strings::table::HEADER_INDEX {
                        table::caption_centered(ui, caption);
                    } else {
                        table::caption(ui, caption);
                    }
                }

                for (i, row) in rows.iter().enumerate() {
                    ui.table_next_row();
                    table::hovered_row(ui, ui.frame_height());

                    ui.table_next_column();
                    table::cell_centered(
                        ui,
                        &format!("##_audidx{}", i),
                        &format!("{}", i + 1),
                        Colors::Secondary,
                    );

                    ui.table_next_column();
                    // Elided like the script paths: a narrow window squeezes this column
                    // before any of the fixed ones.
                    let shown = elide_path(ui, &row.name, table::text_room(ui));
                    table::cell(ui, &format!("##_audname{}", i), &shown, Colors::Success, &row.name);

                    ui.table_next_column();
                    table::cell(ui, &format!("##_audtype{}", i), row.kind, Colors::Text, "");

                    ui.table_next_column();
                    table::cell_parts(
                        ui,
                        &format!("##_audlen{}", i),
                        &[
                            (&row.length, Colors::Text),
                            (" ", Colors::Text),
                            (strings::audio::SECONDS, Colors::Secondary),
                        ],
                        "",
                    );

                    ui.table_next_column();
                    table::cell(ui, &format!("##_audstate{}", i), row.state, Colors::Text, "");

                    ui.table_next_column();
                    table::cell(ui, &format!("##_aud3d{}", i), row.is_3d, Colors::Text, "");

                    ui.table_next_column();
                    table::cell(ui, &format!("##_audloop{}", i), row.looped, Colors::Text, "");

                    ui.table_next_column();
                    table::cell(ui, &format!("##_audvol{}", i), &row.volume, Colors::Text, "");

                    ui.table_next_column();
                    table::cell(ui, &format!("##_audspeed{}", i), &row.speed, Colors::Text, "");
                }
            });
        };

        ui.window(format!(
            "CLEO {} Menu",
            ptr_to_rust_str(unsafe { CLEO_GetVersionStr() })
        ))
        .menu_bar(false)
        .title_bar(false)
        // No `.bg_alpha()`: `WindowBg` carries the opacity, and every child window carries
        // it again in `ChildBg`.
        .size(
            [width, body_height + footer_height + pad * 2.0 + 2.0],
            Condition::Always,
        )
        // The window cannot be moved, so a remembered position only means ending up
        // off-screen after a resolution change: always pin it.
        .position([0.0, 0.0], Condition::Always)
        .resizable(false)
        .movable(false)
        .scroll_bar(false) // the size is computed for the children; a scrollbar here means a layout bug
        .build(|| {
            ui.child_window("main")
                .size([inner_width, body_height])
                .build(|| {
                    // The strip is drawn by hand (see theme::tabs).
                    let clicked = theme::tabs(ui, "main_tabs", &labels, &mut self.tab);
                    match labels[clicked] {
                        strings::tabs::SCRIPTS => {
                            theme::bare_child(ui, "script_tab_inner", || {
                                // The tab was sized for the rows it holds; the table scrolls
                                // inside that if the filter matched more of them.
                                let height = ui.content_region_avail()[1];
                                script_table(ui, &mut self.reload_pending, &mut self.filter, height);
                            });
                        }

                        strings::tabs::LIBRARIES => {
                            if let Some(memory) = plugins::memory() {
                                let num_libs = unsafe { (memory.number_of_libraries)() };
                                if num_libs == 0 {
                                    colored(strings::libraries::EMPTY, Colors::Secondary);
                                } else {
                                    theme::bare_child(ui, "libraries_tab_inner", || {
                                        library_table(ui, ui.content_region_avail()[1]);
                                    });
                                }
                            }
                        }

                        strings::tabs::AUDIO => {
                            if plugins::audio().is_some() {
                                theme::bare_child(ui, "audio_tab_inner", || {
                                    let Some(audio) = plugins::audio() else {
                                        return;
                                    };
                                    if unsafe { (audio.number_of_streams)() } == 0 {
                                        colored(strings::audio::EMPTY, Colors::Secondary);
                                        return;
                                    }
                                    audio_table(ui, ui.content_region_avail()[1]);
                                });
                            }
                        }

                        strings::tabs::LOGS => {
                            // The dropdown switches the tail underneath it; write the pick
                            // back once it changed, so the next load opens the same log.
                            let before = self.log_source;
                            match self.log_source {
                                0 => self
                                    .cleo_log
                                    .render(ui, "cleo_log_inner", &mut self.log_source),
                                _ => self
                                    .script_log
                                    .render(ui, "script_log_inner", &mut self.log_source),
                            }
                            if self.log_source != before {
                                settings::set_log_source(self.log_source);
                            }
                        }

                        strings::tabs::SETTINGS => {
                            theme::bare_child(ui, "settings_tab_inner", || {
                    
                                theme::section(ui, strings::settings::SECTION_APPEARANCE);

                                let mut current = settings::get();
                                let mut changed = false;

                                // Rows are picked by setting, not by label: the labels live in
                                // `strings`, and matching on their text would silently drop a
                                // row whose wording changed.
                                use settings::Setting;
                                let groups: [&[Setting]; 2] = [
                                    &[Setting::Theme, Setting::UiScale, Setting::MenuOpacity],
                                    &[
                                        Setting::ToggleKey,
                                        Setting::FreezeInput,
                                        Setting::ReleaseCursor,
                                        Setting::ShowFps,
                                        Setting::OpenOnStart,
                                    ],
                                ];

                                for (index, group) in groups.iter().enumerate() {
                                    if index > 0 {
                                        theme::section(ui, strings::settings::SECTION_BEHAVIOUR);
                                    }
                                    for def in settings::SCHEMA {
                                        if group.contains(&def.setting) {
                                            changed |= settings::row(ui, &mut current, def);
                                        }
                                    }
                                }

                                if changed {
                                    settings::set(current);
                                }

                                theme::section(ui, strings::settings::SECTION_STORAGE);
                                settings::labeled(ui, strings::settings::LABEL_SETTINGS_FILE);
                                if let Some(_mono) = theme::mono_font(ui) {
                                    colored(
                                        &settings_file_path().display().to_string(),
                                        Colors::Secondary,
                                    );
                                }
                            });

                        }
                        _ => {}
                    }
                });

            ui.child_window("footer")
                .size([inner_width, footer_height])
                .build(|| {
                // Text sits next to a framed button, so it has to be shifted down into the
                // frame padding or the two baselines disagree.
                ui.align_text_to_frame_padding();
                colored(strings::footer::VERSION_LABEL, Colors::Secondary);
                ui.same_line();
                ui.align_text_to_frame_padding();
                colored(
                    &format!("{}", ptr_to_rust_str(unsafe { CLEO_GetVersionStr() })),
                    Colors::Text,
                );
                ui.same_line();

                if theme::flat_button(ui, strings::footer::CHECK_UPDATES) {
                    use config::Config;
                    use github::fetch_latest_release;
                    use version::should_update;
                    let config = Config::default();
                    let api_url = config.get_github_releases_url();

                    unsafe { UPDATE_STATUS = 1 };

                    if let Ok(release) = fetch_latest_release(&api_url) {
                        let current_version = ptr_to_rust_str(unsafe { CLEO_GetVersionStr() });
                        let new_version = release.get_version();
                        unsafe { UPDATE_STATUS = 3 };
                        if let Ok(should_update) = should_update(&current_version, &new_version) {
                            if should_update {
                                unsafe { UPDATE_STATUS = 2 };
                            }
                        }
                    }
                }
                if unsafe { UPDATE_STATUS > 0 } {
                    ui.same_line();
                    ui.align_text_to_frame_padding();

                    let text = match unsafe { UPDATE_STATUS } {
                        1 => strings::footer::CHECKING,
                        2 => strings::footer::UPDATE_AVAILABLE,
                        3 => strings::footer::UP_TO_DATE,
                        _ => "",
                    };
                    colored(text, Colors::Success);
                }
                ui.same_line();

                if settings::get().show_fps {
                    ui.align_text_to_frame_padding();
                    colored(strings::footer::FPS_LABEL, Colors::Secondary);
                    ui.same_line();
                    ui.align_text_to_frame_padding();
                    colored(&format!("{:.0}", ui.io().framerate), Colors::Text);
                }

                colored(
                    &strings::footer::close_hint(&settings::get().toggle_key_label()),
                    Colors::Secondary,
                );
            });
        });
    }
}

#[ctor]
fn main() {
    use hudhook::hooks::dx9::ImguiDx9Hooks;
    hudhook!(ImguiDx9Hooks, Dashboard::default());
}
