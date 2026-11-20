//! Tail view for CLEO's logs, with tracing controls and a freezable auto-scroll. Only the last
//! few hundred KiB are re-read, and at most every [`REFRESH_MS`].

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use imgui::{StyleVar, Ui};

use crate::icons;
use crate::highlight;
use crate::modules;
use crate::sdk::CLEO_GetLogDirectory;
use crate::settings;
use crate::strings;
use crate::theme;

/// Bytes read from the end of the file on each refresh.
const TAIL_BYTES: u64 = 512 * 1024;

/// Lines kept after reading.
const TAIL_LINES: usize = 2000;

/// Minimum time between two disk reads, in milliseconds.
const REFRESH_MS: u32 = 500;

/// The files the Logs tab can show, in dropdown order; the second element is the
/// tooltip.
pub const LOG_SOURCES: [(&str, &str); 2] = [
    (strings::logs::SOURCE_CLEO, strings::logs::TIP_SOURCE_CLEO),
    (
        strings::logs::SOURCE_SCRIPT,
        strings::logs::TIP_SOURCE_SCRIPT,
    ),
];

/// Dim colour of the active theme.
fn hint_color() -> [f32; 4] {
    theme::dim_color().to_rgba_f32s()
}

pub struct LogTail {
    path: PathBuf,
    /// Oldest line first, like the file.
    lines: Vec<String>,
    /// `Some(hint)` when the file could not be read.
    status: Option<String>,
    last_refresh: u32,
    /// Keep the newest line in view; the user can turn it off to read history.
    follow: bool,
    /// The view reached the end at least once during the current follow run.
    pinned: bool,
    /// Only the script log has the CLEO script-tracing controls.
    tracing: bool,
    /// What the shown lines must contain. Empty shows the whole tail.
    filter: String,
    /// A restart of the plugin's logging, in flight. See [`LogTail::restart_logging`].
    restart: Option<Restart>,
}

/// Logging turned off for a frame, to be turned back on.
///
/// The DebugUtils plugin re-reads its switches only when the *logging state* changes, so a
/// lone key write is never picked up. Off and back on forces a re-read, but the two writes
/// cannot share a frame: the second would overwrite the pending state the first queued.
struct Restart {
    /// The level to restore.
    level: i32,
    /// Frames still to wait. The plugin applies a queued state at the end of the frame it was
    /// queued in, so one frame is the minimum and two is the margin.
    wait: u8,
}

impl LogTail {
    /// Build a tail for `file_name` inside CLEO's log directory.
    pub fn new(file_name: &str) -> Self {
        let dir = unsafe { CLEO_GetLogDirectory() };
        let dir = if dir.is_null() {
            ".".to_string()
        } else {
            unsafe { std::ffi::CStr::from_ptr(dir) }
                .to_string_lossy()
                .into_owned()
        };

        Self {
            path: Path::new(&dir).join(file_name),
            lines: Vec::new(),
            status: None,
            last_refresh: 0,
            follow: true,
            pinned: false,
            tracing: true,
            filter: String::new(),
            restart: None,
        }
    }

    /// A log that has no tracing controls of its own (`cleo.log`).
    pub fn plain(file_name: &str) -> Self {
        let mut tail = Self::new(file_name);
        tail.tracing = false;
        tail
    }

    /// Restart the plugin's logging with the settings as they are now. Off and back on, a
    /// frame apart: see [`Restart`]. The plugin flushes what it recorded and starts a new
    /// session in the same file, so nothing already logged is lost.
    fn restart_logging(&mut self) {
        let level = match self.restart.as_ref() {
            // Already off and on its way back: keep the level it is heading for, and give
            // the change that just arrived time to be written before the log comes up.
            Some(pending) => pending.level,
            None => {
                let level = settings::tracing_level();
                // Nothing is being written, and the next start reads the settings anyway.
                if level == 0 {
                    return;
                }
                settings::set_tracing_level(0);
                level
            }
        };
        self.restart = Some(Restart { level, wait: 2 });
    }

    /// Finish a restart in flight. Called every frame the menu draws, not just while this log
    /// is on screen: the level is off the whole time it runs.
    pub fn poll(&mut self) {
        let Some(restart) = self.restart.as_mut() else {
            return;
        };
        if restart.wait > 0 {
            restart.wait -= 1;
            return;
        }
        let level = restart.level;
        self.restart = None;
        settings::set_tracing_level(level);
    }

    /// Re-read the tail, at most every [`REFRESH_MS`]. Does nothing while the user froze the
    /// view: the buffer is a sliding window over the end of the file, so a refresh would
    /// shift the lines under the cursor. "Jump to latest" resyncs it.
    pub fn refresh(&mut self) {
        if !self.follow {
            return;
        }

        let now = unsafe { GetTickCount() };
        if now.wrapping_sub(self.last_refresh) < REFRESH_MS && self.status.is_some() {
            return;
        }
        self.last_refresh = now;

        match read_tail(&self.path, TAIL_BYTES, TAIL_LINES) {
            Ok(lines) => {
                self.lines = lines;
                self.status = None;
            }
            Err(err) => {
                self.lines.clear();
                if err.kind() == std::io::ErrorKind::NotFound && !self.tracing {
                    self.status =
                        Some(strings::logs::not_written(&self.path.display().to_string()));
                    return;
                }
                self.status = Some(if err.kind() == std::io::ErrorKind::NotFound {
                    strings::logs::NO_LOG_YET.to_string()
                } else {
                    err.to_string()
                });
            }
        }
    }

    /// Drop the refresh interval, so the next [`refresh`](Self::refresh) reads the file even
    /// if one just happened. Used when the tab showing the log comes back on screen.
    pub fn invalidate(&mut self) {
        self.last_refresh = 0;
    }

    /// The file this tail reads, which the footer names while the Logs tab is up.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Toolbar (which log, tracing level, auto-scroll) plus the scrollable log body.
    ///
    /// `source` indexes [`LOG_SOURCES`]. `scroll` is the key's move in lines, positive down.
    /// `focus` is the frame's `/`, which puts the keyboard in the search box.
    pub fn render(
        &mut self,
        ui: &Ui,
        id: &str,
        source: &mut usize,
        scroll: Option<f32>,
        focus: bool,
    ) {
        // Asked of every line twice - once for the count and once while drawing. A byte scan
        // is cheaper than the allocation a pre-filtered list would need every frame.
        let needle = self.filter.trim().to_string();
        let needle = (!needle.is_empty()).then_some(needle);
        let shown = match needle.as_deref() {
            Some(needle) => self
                .lines
                .iter()
                .filter(|line| highlight::contains(line, needle))
                .count(),
            None => self.lines.len(),
        };

        let mut config = false;

        let card_id = format!("{}_toolbar", id);
        // Two rows of controls, each a frame-height widget, plus the gap between them and
        // the card's padding. The log's path is in the footer, not here.
        let card_height =
            ui.frame_height() * 2.0 + ui.clone_style().item_spacing[1] + 8.0;
        theme::card_fixed(ui, card_id.as_str(), card_height, || {
            let followed = self.follow;

            // Which file: both share this toolbar instead of getting a tab each.
            let selected = LOG_SOURCES.get(*source).map(|s| s.0).unwrap_or("");
            let source_tip = LOG_SOURCES
                .get(*source)
                .map(|(_, hint)| *hint)
                .unwrap_or("");
            theme::dropdown(
                ui,
                &format!("{}_source", id),
                selected,
                source_tip,
                || {
                    for (index, (label, _)) in LOG_SOURCES.iter().enumerate() {
                        if ui
                            .selectable_config(label)
                            .selected(*source == index)
                            .build()
                        {
                            *source = index;
                        }
                    }
                },
            );
            // Spacing, not a separator: a line separator breaks the row in two.
            ui.same_line_with_spacing(0.0, 16.0);

            let follow_id = format!("Auto-scroll##{}_follow", id);
            theme::switch(ui, follow_id.as_str(), &mut self.follow);
            theme::hint(ui, strings::logs::TIP_AUTO_SCROLL);
            ui.same_line();
            ui.align_text_to_frame_padding();
            // The switch is the control; the word beside it is its caption.
            ui.text_colored(hint_color(), strings::logs::AUTO_SCROLL);
            ui.same_line();

            let jump_id = format!("{}##{}_jump", strings::logs::JUMP_TO_LATEST, id);
            if theme::flat_button(ui, jump_id.as_str()) {
                self.follow = true;
                self.pinned = false;
            }
            theme::hint(ui, strings::logs::TIP_JUMP_TO_LATEST);

            // Drop the cached lines, or the stale scroll offset of the previous, frozen
            // run cancels the re-armed follow and the view does not jump.
            if self.follow && !followed {
                self.pinned = false;
                self.last_refresh = 0;
            }

            if self.tracing {
                // Only the script log has these, so the `same_line` belongs in this branch:
                // on `cleo.log` it would put the search box at the end of this row.
                ui.same_line();

                // The settings are the DebugUtils plugin's: it writes `cleo_script.log` and
                // is the only listener for those keys, so without it the button is offered
                // disabled and names the missing module.
                let available = modules::is_loaded(strings::logs::TRACING_PLUGIN);
                let _disabled = ui.begin_disabled(!available);

                ui.text_colored(hint_color(), strings::logs::TRACING_LABEL);
                ui.same_line();
                // The level in force, as a reading: the control that changes it is the
                // button, and it lives in the popup with the rest of the keys.
                ui.text_colored(theme::text_color().to_rgba_f32s(), settings::tracing_label(settings::tracing_level()));
                ui.same_line_with_spacing(0.0, 8.0);

                let icon = (ui.text_line_height() * 1.05).round();
                if theme::icon_button(
                    ui,
                    "##log_config",
                    icons::Icon::Sliders,
                    icon,
                    theme::accent_color(),
                ) {
                    config = true;
                }
                theme::hint(ui, strings::logs::TIP_CONFIG);

                drop(_disabled);
                // Only when there is something to say beside it: an unconditional
                // `same_line` here left the cursor on this row, and the next row's search
                // box was drawn at the end of this one.
                if !available {
                    ui.same_line();
                    ui.align_text_to_frame_padding();
                    ui.text_colored(hint_color(), strings::logs::TRACING_NO_PLUGIN);
                }
            }

            // Second row: the search box has the width of the whole card to itself, and the
            // count of what it left is beside it rather than at the end of a crowded row.
            filter_box(ui, &format!("{}_filter", id), &mut self.filter, focus);
            if needle.is_some() {
                ui.same_line_with_spacing(0.0, 16.0);
                ui.align_text_to_frame_padding();
                ui.text_colored(hint_color(), strings::logs::matching(shown, self.lines.len()));
            }
        });

        if config {
            ui.open_popup("log_config");
        }
        if config_popup(ui, id) {
            self.restart_logging();
        }

        ui.separator();

        let _mono = theme::mono_font(ui);
        // The menu sheet behind it is already faded; no second background.
        theme::padded_child(ui, id, || {
            if let Some(status) = &self.status {
                ui.text_colored(hint_color(), status);
                return;
            }

            if self.lines.is_empty() {
                ui.text_colored(hint_color(), strings::logs::EMPTY);
                return;
            }

            // Scrolling up is a decision to read history, so it leaves auto-scroll - the pin
            // at the bottom would otherwise pull it straight back down.
            if let Some(rows) = scroll {
                if rows < 0.0 {
                    self.follow = false;
                    self.pinned = false;
                }
                if !self.follow {
                    let line = ui.text_line_height() + ui.clone_style().item_spacing[1];
                    let target = (ui.scroll_y() + rows * line).clamp(0.0, ui.scroll_max_y());
                    ui.set_scroll_y(target);
                }
            }

            let colors = highlight::Colors::theme();
            let mut painter = highlight::Painter::default();
            let syntax = settings::highlight();

            // Colour only the lines the view can see: a syntax run per line is too much work
            // for rows that are clipped away. Every line is still laid out, or the scrollbar
            // would have nothing to measure.
            let view = ui.window_pos()[1]..ui.window_pos()[1] + ui.window_size()[1];
            let pitch = ui.text_line_height() + ui.clone_style().item_spacing[1];
            let margin = pitch * 2.0;

            let mut drawn = 0usize;
            for line in &self.lines {
                if needle
                    .as_deref()
                    .is_some_and(|needle| !highlight::contains(line, needle))
                {
                    continue;
                }
                let top = ui.cursor_screen_pos()[1];
                let visible = top + pitch >= view.start - margin && top <= view.end + margin;
                painter.draw(ui, line, needle.as_deref(), &colors, syntax && visible);
                drawn += 1;
            }

            if drawn == 0 {
                ui.text_colored(hint_color(), strings::logs::NO_MATCH);
            }

            // Following means pinning to the newest line. Any manual scroll cancels it, so
            // the wheel works right away instead of fighting the pin every frame.
            if self.follow {
                let at_end = ui.scroll_max_y() - ui.scroll_y() < 2.0;
                // Only scrolling up takes the log out of follow: a wheel down at the
                // bottom asks for more of what is already there.
                let wheel = crate::theme::scrolled_up(ui);

                if wheel || (self.pinned && !at_end) {
                    self.follow = false;
                    self.pinned = false;
                } else {
                    ui.set_scroll_here_y_with_ratio(1.0);
                    self.pinned = true;
                }
            }
        });
    }
}

/// The script log's settings, behind the toolbar's slider button: the DebugUtils plugin's
/// keys, level first. The note at the top says once that they restart the log.
fn config_popup(ui: &Ui, id: &str) -> bool {
    let Some(_popup) = ui.begin_popup("log_config") else {
        return false;
    };

    // Whether a key the plugin only reads when logging *starts* was touched. The level is
    // not one of them: the plugin watches that key and switches itself.
    let mut changed = false;

    ui.text_colored(hint_color(), strings::logs::LOG_CONFIG_NOTE);
    ui.separator();

    // One column for the captions, measured from the widest of them: six rows of a popup
    // are only readable if their controls start at the same x.
    let column = [
        strings::logs::TRACING_LABEL,
        strings::logs::LOG_MAX_SIZE,
        strings::logs::LOG_CUSTOM_SCRIPTS,
        strings::logs::LOG_DEBUG_SCRIPTS,
        strings::logs::LOG_OFFSETS,
        strings::logs::LOG_OPCODES,
    ]
    .iter()
    .map(|label| ui.calc_text_size(label)[0])
    .fold(0.0_f32, f32::max)
        + 16.0;

    let level = settings::tracing_level();
    config_row(ui, column, strings::logs::TRACING_LABEL, |ui| {
        theme::dropdown(
            ui,
            &format!("{}_level", id),
            settings::tracing_label(level),
            strings::logs::TIP_TRACING,
            || {
                for (label, value) in settings::TRACING_LEVELS {
                    if ui.selectable_config(label).selected(level == value).build() {
                        settings::set_tracing_level(value);
                    }
                }
            },
        );
    });

    let mut size = settings::log_max_size();
    config_row(ui, column, strings::logs::LOG_MAX_SIZE, |ui| {
        // Wide enough for the number and the two step buttons: at 120 the field showed
        // `2!` - the value cut off behind the buttons - for the default of 250.
        ui.set_next_item_width(170.0);
        if ui
            .input_int(format!("##{}_max_size", id), &mut size)
            .step(10)
            .step_fast(100)
            .build()
        {
            settings::set_log_max_size(size);
            changed = true;
        }
        theme::hint(ui, strings::logs::TIP_LOG_MAX_SIZE);
    });

    let scope = settings::log_custom_only();
    config_row(ui, column, strings::logs::LOG_CUSTOM_SCRIPTS, |ui| {
        theme::dropdown(
            ui,
            &format!("{}_scope", id),
            settings::log_scope_label(scope),
            strings::logs::TIP_LOG_CUSTOM_SCRIPTS,
            || {
                for (label, value) in settings::LOG_SCOPE_LEVELS {
                    if ui.selectable_config(label).selected(scope == value).build() {
                        settings::set_log_custom_only(value);
                        changed = true;
                    }
                }
            },
        );
    });

    let mut debug_only = settings::log_debug_only();
    config_row(ui, column, strings::logs::LOG_DEBUG_SCRIPTS, |ui| {
        if theme::switch(ui, format!("##{}_debug_only", id), &mut debug_only) {
            settings::set_log_debug_only(debug_only);
            changed = true;
        }
        theme::hint(ui, strings::logs::TIP_LOG_DEBUG_SCRIPTS);
    });

    let mut offsets = settings::log_offsets();
    config_row(ui, column, strings::logs::LOG_OFFSETS, |ui| {
        if theme::switch(ui, format!("##{}_offsets", id), &mut offsets) {
            settings::set_log_offsets(offsets);
            changed = true;
        }
        theme::hint(ui, strings::logs::TIP_LOG_OFFSETS);
    });

    let mut opcodes = settings::log_opcodes();
    config_row(ui, column, strings::logs::LOG_OPCODES, |ui| {
        if theme::switch(ui, format!("##{}_opcodes", id), &mut opcodes) {
            settings::set_log_opcodes(opcodes);
            changed = true;
        }
        theme::hint(ui, strings::logs::TIP_LOG_OPCODES);
    });

    changed
}

/// One row of [`config_popup`]: a dim caption in the column, then the control.
fn config_row(ui: &Ui, column: f32, label: &str, control: impl FnOnce(&Ui)) {
    let start = ui.cursor_pos()[0];
    ui.align_text_to_frame_padding();
    ui.text_colored(hint_color(), label);
    ui.same_line_with_pos(start + column);
    control(ui);
}

/// The log toolbar's filter box, drawn the way the Scripts and Libraries toolbars draw
/// theirs: a frame padded to leave the magnifier room, with the icon painted over it.
fn filter_box(ui: &Ui, id: &str, filter: &mut String, focus: bool) {
    let style = ui.clone_style();
    let icon = (ui.text_line_height() * 0.95).round();
    // A row of its own: wide enough to read a path or an opcode name back in one look.
    let width = (ui.content_region_avail()[0] * 0.45).clamp(220.0, 480.0);

    let _pad = ui.push_style_var(StyleVar::FramePadding([
        icon + 12.0,
        style.frame_padding[1],
    ]));
    // `/` puts the keyboard in the box, the way it does on the Scripts and Libraries tabs.
    if focus {
        ui.set_keyboard_focus_here();
    }
    ui.set_next_item_width(width);
    // `##` keeps the id off the screen: `InputText` draws its label to the right of the
    // field otherwise, and this one is an id, not a caption.
    ui.input_text(format!("##{id}"), filter)
        .hint(strings::logs::SEARCH_PLACEHOLDER)
        .build();
    let min = ui.item_rect_min();
    let max = ui.item_rect_max();
    icons::draw(
        &ui.get_window_draw_list(),
        icons::Icon::Search,
        [min[0] + 9.0, (min[1] + max[1]) * 0.5 - icon * 0.5],
        icon,
        theme::dim_color(),
    );

    // Only until there is something to narrow: a tip over the box the user is typing in
    // would sit on top of the text they are reading back.
    if filter.is_empty() {
        theme::hint(ui, strings::logs::TIP_SEARCH);
    }
}

/// Read the last `max_bytes` of `path`, keep `max_lines` lines, oldest first.
fn read_tail(path: &Path, max_bytes: u64, max_lines: usize) -> std::io::Result<Vec<String>> {
    let mut file = std::fs::File::open(path)?;
    let len = file.metadata()?.len();
    let start = len.saturating_sub(max_bytes);

    file.seek(SeekFrom::Start(start))?;
    let mut buf = Vec::new();
    file.take(max_bytes).read_to_end(&mut buf)?;

    let text = String::from_utf8_lossy(&buf);
    let mut lines: Vec<String> = text.lines().map(|line| line.to_string()).collect();

    // The first line of a partial read is almost certainly cut in half.
    if start > 0 && !lines.is_empty() {
        lines.remove(0);
    }

    if lines.len() > max_lines {
        lines.drain(..lines.len() - max_lines);
    }

    Ok(lines)
}

#[cfg(windows)]
extern "system" {
    fn GetTickCount() -> u32;
}
