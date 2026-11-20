//! The Logs page: one log tail at a time, with the picker and the tracing controls.

use imgui::Ui;

use crate::logtail::LogTail;
use crate::nav::Intent;
use crate::settings;

pub struct State {
    /// CLEO's own `cleo.log`.
    cleo: LogTail,
    /// `cleo_script.log`, written by the DebugUtils plugin.
    script: LogTail,
    /// Which log is shown, as an index into [`crate::logtail::LOG_SOURCES`].
    pub source: usize,
    /// Whether the page was drawn last frame, so coming back to it re-reads at once instead
    /// of waiting out the refresh interval.
    on_screen: bool,
}

impl State {
    pub fn new() -> Self {
        Self {
            cleo: LogTail::plain("cleo.log"),
            script: LogTail::new("cleo_script.log"),
            source: settings::log_source(),
            on_screen: false,
        }
    }

    pub fn active(&self) -> &LogTail {
        if self.source == 0 {
            &self.cleo
        } else {
            &self.script
        }
    }

    /// Only the log on screen is re-read, and only while the Logs tab is up.
    pub fn refresh(&mut self, visible: bool) {
        if visible {
            if !self.on_screen {
                if self.source == 0 {
                    self.cleo.invalidate();
                } else {
                    self.script.invalidate();
                }
            }
            if self.source == 0 {
                self.cleo.refresh();
            } else {
                self.script.refresh();
            }
        }
        self.on_screen = visible;
    }

    /// Finishes a logging restart in flight. Runs every frame the menu draws, not only while
    /// this log is on screen: the level is off the whole time it takes.
    pub fn poll(&mut self) {
        self.cleo.poll();
        self.script.poll();
    }

    pub fn page(&mut self, ui: &Ui, intent: Intent, page_rows: i32) {
        let before = self.source;
        // The arrows walk the view by a line, `PageUp` / `PageDown` by a screenful.
        let scroll = intent
            .row
            .map(|movement| movement.rows(page_rows) as f32);

        let (tail, id) = if self.source == 0 {
            (&mut self.cleo, "cleo_log_inner")
        } else {
            (&mut self.script, "script_log_inner")
        };
        tail.render(ui, id, &mut self.source, scroll, intent.filter);

        if self.source != before {
            settings::set_log_source(self.source);
        }
    }
}
