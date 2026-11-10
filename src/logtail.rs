//! Tail view for CLEO's logs, with tracing controls and a freezable auto-scroll.
//!
//! Only the last few hundred KiB are re-read, and only every
//! [`REFRESH_MS`]: the menu renders at 30 fps and a multi-megabyte log would stutter.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use imgui::Ui;

use crate::sdk::CLEO_GetLogDirectory;
use crate::settings;
use crate::strings;
use crate::theme;

/// Bytes read from the end of the file on each refresh.
const TAIL_BYTES: u64 = 512 * 1024;

/// Lines kept after reading.
const TAIL_LINES: usize = 4000;

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
        }
    }

    /// A log that has no tracing controls of its own (`cleo.log`).
    pub fn plain(file_name: &str) -> Self {
        let mut tail = Self::new(file_name);
        tail.tracing = false;
        tail
    }

    /// Re-read the tail, at most every [`REFRESH_MS`].
    ///
    /// Does nothing while the user froze the view: the buffer is a sliding window over
    /// the end of the file, so refreshing it would shift the lines under the cursor.
    /// "Jump to latest" resyncs it.
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

    /// Toolbar (which log, tracing level, auto-scroll) plus the scrollable log body.
    ///
    /// `source` indexes [`LOG_SOURCES`].
    pub fn render(&mut self, ui: &Ui, id: &str, source: &mut usize) {
        let card_id = format!("{}_toolbar", id);
        // A card sized to the bare text height clips the bottom off every control on the
        // row, so leave room for a frame-height widget.
        let card_height = ui.frame_height() + 8.0;
        theme::card(ui, card_id.as_str(), card_height, || {
            let followed = self.follow;

            // Which file: both share this toolbar instead of getting a tab each.
            let source_id = format!("##{}_source", id);
            let selected = LOG_SOURCES.get(*source).map(|s| s.0).unwrap_or("");
            ui.set_next_item_width(160.0);
            if let Some(_token) = ui.begin_combo(source_id.as_str(), selected) {
                for (index, (label, _)) in LOG_SOURCES.iter().enumerate() {
                    if ui
                        .selectable_config(label)
                        .selected(*source == index)
                        .build()
                    {
                        *source = index;
                    }
                }
            }
            if let Some((_, hint)) = LOG_SOURCES.get(*source) {
                theme::hint(ui, hint);
            }
            // Spacing, not a separator: a line separator breaks the row in two.
            ui.same_line_with_spacing(0.0, 16.0);

            let follow_id = format!("Auto-scroll##{}_follow", id);
            theme::switch(ui, follow_id.as_str(), &mut self.follow);
            theme::hint(ui, strings::logs::TIP_AUTO_SCROLL);
            ui.same_line();
            ui.align_text_to_frame_padding();
            ui.text(strings::logs::AUTO_SCROLL);
            ui.same_line();

            let jump_id = format!("{}##{}_jump", strings::logs::JUMP_TO_LATEST, id);
            if theme::flat_button(ui, jump_id.as_str()) {
                self.follow = true;
                self.pinned = false;
            }
            theme::hint(ui, strings::logs::TIP_JUMP_TO_LATEST);
            ui.same_line();

            // Drop the cached lines, or the stale scroll offset of the previous, frozen
            // run cancels the re-armed follow and the view does not jump.
            if self.follow && !followed {
                self.pinned = false;
                self.last_refresh = 0;
            }

            if !self.follow {
                ui.text_colored(hint_color(), strings::logs::FROZEN);
                ui.same_line();
            }

            if self.tracing {
                ui.text_colored(hint_color(), strings::logs::TRACING_LABEL);
                ui.same_line();
                ui.set_next_item_width(150.0);

                let level = settings::tracing_level();
                let combo_id = format!("##{}_tracing", id);
                if let Some(_token) =
                    ui.begin_combo(combo_id.as_str(), settings::tracing_label(level))
                {
                    for (label, value) in settings::TRACING_LEVELS {
                        if ui.selectable_config(label).selected(level == value).build() {
                            settings::set_tracing_level(value);
                        }
                    }
                }
                theme::hint(ui, strings::logs::TIP_TRACING);

                ui.same_line();
            }

            if let Some(_mono) = theme::mono_font(ui) {
                // Elide the head of the path: it is usually a long prefix that says
                // nothing, and a full path pushes the row out of the card.
                let full = self.path.display().to_string();
                let char_width = ui.calc_text_size("W")[0].max(1.0);
                let room = (ui.content_region_avail()[0] / char_width).floor() as usize;
                let length = full.chars().count();
                let shown = if length > room && room > 8 {
                    let tail: String = full.chars().skip(length - (room - 3)).collect::<String>();
                    format!("...{tail}")
                } else {
                    full.clone()
                };

                ui.text_colored(hint_color(), shown);
                if ui.is_item_hovered() {
                    ui.tooltip_text(full);
                }
            }
        });

        ui.separator();

        let _mono = theme::mono_font(ui);
        // The menu sheet behind it is already faded; no second background.
        theme::bare_child(ui, id, || {
            if let Some(status) = &self.status {
                ui.text_colored(hint_color(), status);
                return;
            }

            if self.lines.is_empty() {
                ui.text_colored(hint_color(), strings::logs::EMPTY);
                return;
            }

            for line in &self.lines {
                ui.text(line);
            }

            // Following means pinning to the newest line. Any manual scroll cancels it, so
            // the wheel works right away instead of fighting the pin every frame.
            if self.follow {
                let at_end = ui.scroll_max_y() - ui.scroll_y() < 2.0;
                let wheel = ui.io().mouse_wheel != 0.0 && ui.is_window_hovered();

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
