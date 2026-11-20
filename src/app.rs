//! The menu: the state the render loop keeps between frames, and the frame that draws the
//! sheet and hands this frame's keys to the page that is up.

use hudhook::windows::Win32::System::SystemInformation::GetTickCount;
use hudhook::*;
use imgui::{Condition, Context, StyleVar, Ui};

use crate::chrome;
use crate::debug;
use crate::icons;
use crate::nav::{Intent, Nav};
use crate::overlay;
use crate::pages::{self, Report};
use crate::panic_log;
use crate::plugins;
use crate::settings;
use crate::sdk::CLEO_GetVersionStr;
use crate::strings;
use crate::theme;
use crate::threads;
use crate::utils::{async_key_down, ptr_to_rust_str, settings_file_path};

/// Minimum time between two accepted hotkey presses. A single tap can show up as two
/// down-edges (key repeat, a remapper), which used to open the menu and close it again.
const TOGGLE_COOLDOWN_MS: u32 = 250;

/// Rising-edge detector for the menu hotkey, read straight from the keyboard state so it
/// works while the game or the menu is busy, independently of ImGui input.
struct ToggleKeyState {
    /// The key this tracker belongs to, so switching the hotkey mid-session cannot report the
    /// old key's state as a fresh press.
    key: i32,
    was_down: bool,
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

/// Everything one frame of the sheet needs to know about the screen and the page it draws.
struct Layout {
    size: [f32; 2],
    /// Width of the sheet's content region: the title row, the body and the footer all end
    /// where the sheet's own padding ends.
    inner_width: f32,
    inset: f32,
    /// The tab strip, in on-screen order.
    tabs: Vec<(&'static str, icons::Icon)>,
    names: Vec<&'static str>,
    body_height: f32,
    main_width: f32,
    /// The info panel's width, when it is drawn.
    sidebar_width: Option<f32>,
    gap: f32,
    /// Rows a pageful holds, which `PageUp` / `PageDown` move by.
    page_rows: i32,
    footer_height: f32,
    chrome_height: f32,
    /// The left button's rising edge this frame.
    click: bool,
}

pub struct Dashboard {
    toggle: ToggleKeyState,
    /// Theme and opacity the ImGui style was last built for.
    style: Option<(i32, f32)>,
    /// Visible tab. Kept here because the strip is drawn by hand.
    tab: usize,
    logs: pages::logs::State,
    scripts: pages::scripts::State,
    libraries: pages::libraries::State,
    /// Rows the table on the page being drawn actually drew, which is what the footer counts.
    rows_shown: usize,
    /// Height the title row and the tab strip took on the last frame, measured from the top of
    /// the sheet. Zero until the first frame has drawn them.
    chrome_above: f32,
    /// How much taller or shorter the sheet has to be than its children ask for, so that it
    /// settles on the right height instead of hanging off the screen.
    sheet_slack: f32,
    /// Whether the left button was down last frame: ImGui keeps its click flag private.
    mouse_down: bool,
    nav: Nav,
    /// Row cursor of the pages that have no selection of their own (Libraries, Audio).
    nav_row: Option<usize>,
}

impl Default for Dashboard {
    fn default() -> Self {
        Self {
            toggle: ToggleKeyState::new(),
            style: None,
            tab: 0,
            logs: pages::logs::State::new(),
            scripts: pages::scripts::State::default(),
            libraries: pages::libraries::State::default(),
            rows_shown: 0,
            chrome_above: 0.0,
            sheet_slack: 0.0,
            mouse_down: false,
            nav: Nav::new(),
            nav_row: None,
        }
    }
}

/// Tabs in on-screen order, with the icon drawn in front of each. Libraries needs no plugin;
/// Audio is only there when `SA.Audio.cleo` is, because every row of it comes from there.
fn tabs() -> Vec<(&'static str, icons::Icon)> {
    let mut items = vec![
        (strings::tabs::SCRIPTS, icons::Icon::Play),
        (strings::tabs::LIBRARIES, icons::Icon::Puzzle),
    ];
    if plugins::audio().is_some() {
        items.push((strings::tabs::AUDIO, icons::Icon::Speaker));
    }
    items.push((strings::tabs::LOGS, icons::Icon::Journal));
    items.push((strings::tabs::SETTINGS, icons::Icon::Gear));
    items
}

/// The tab the strip currently shows, tolerant of the Audio tab coming and going.
fn current_page(tab: usize) -> &'static str {
    tabs()
        .get(tab)
        .map(|(label, _)| *label)
        .unwrap_or(strings::tabs::SCRIPTS)
}

impl ImguiRenderLoop for Dashboard {
    fn initialize(&mut self, ctx: &mut Context, _render_context: &mut dyn RenderContext) {
        // Before anything that could panic: the file has to exist from the first frame on.
        panic_log::install();
        log::info!("Imgui initialized");
        theme::register_fonts(ctx);

        let current = settings::get();
        theme::set_current(theme::Theme::from_int(current.theme));
        theme::apply(ctx);
        self.style = Some((current.theme, current.menu_opacity));

        // No ImGui layout file: the window is pinned and sized from the content.
        ctx.set_ini_filename(None);

        settings::apply(current);
        ctx.io_mut().font_global_scale = current.ui_scale;

        if !overlay::install_iat_hooks() {
            log::warn!("could not redirect USER32 ClipCursor/SetCursorPos imports");
        }
        if !overlay::install_input_freeze_hook() {
            log::warn!("could not detour the mouse poll; Freeze game input will not stop mouse look");
        }

        // Opcode callbacks for the per-script debug view. They return immediately unless a
        // script is being traced, so they cost one atomic load while nobody is debugging.
        debug::install();

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
            overlay::set_open(!overlay::is_open());
        }

        // Theme switches and the opacity slider land in the same style.
        theme::set_opacity(current.menu_opacity);
        let style = (current.theme, current.menu_opacity);
        if self.style != Some(style) {
            theme::set_current(theme::Theme::from_int(current.theme));
            theme::apply(ctx);
            self.style = Some(style);
        }

        let io = ctx.io_mut();
        io.mouse_draw_cursor = overlay::is_open();
        if io.font_global_scale != current.ui_scale {
            io.font_global_scale = current.ui_scale;
        }

        // Frame stamp for the trace. A park outlives the menu: a script paused in the
        // debugger stays paused until the Scripts table resumes it.
        debug::tick();
        plugins::detect();

        // Only the log on screen is re-read, and only while the Logs tab is up.
        let logs_visible = overlay::is_open() && current_page(self.tab) == strings::tabs::LOGS;
        self.logs.refresh(logs_visible);
        // A logging restart runs over two frames and the level is off for both of them, so it
        // has to finish wherever the menu is.
        self.logs.poll();

        if overlay::is_open() && current.freeze_input {
            unsafe { overlay::zero_mouse_delta() };
        }
    }

    /// Stops the game window receiving input while the menu is open. ImGui still gets it:
    /// hudhook feeds the window procedure first.
    fn message_filter(&self, _io: &imgui::Io) -> MessageFilter {
        if overlay::is_open() && settings::get().freeze_input {
            MessageFilter::InputAll
        } else {
            MessageFilter::empty()
        }
    }

    fn render(&mut self, ui: &mut imgui::Ui) {
        // A panic in here unwinds into hudhook's `extern "system"` detour, where Rust aborts
        // the process. Catching it keeps the game and puts the reason in the panic log.
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.render_frame(ui)))
            .is_err()
        {
            // The frame stopped half drawn: close the sheet so the next open starts clean
            // rather than repeating the panic every frame.
            overlay::set_open(false);
        }
    }
}

impl Dashboard {
    fn render_frame(&mut self, ui: &mut Ui) {
        // The keys are read on every frame, up or not: gathering them keeps their held state
        // current, and the sheet primes them the frame it comes up.
        let open = overlay::is_open();
        let typing = ui.io().want_text_input || ui.is_any_item_active();
        let intent = self.nav.poll(open, typing, settings::get().toggle_key);
        if !open {
            return;
        }

        if intent.close && !debug::close_focused() {
            // Escape backs out of one window at a time: the floating window over the sheet
            // first, the sheet second.
            overlay::set_open(false);
            return;
        }

        // A floating window on top owns the keys: while a debugger is up, Tab and the arrows
        // are its panes and its trace list, and the sheet must not move with the same press.
        let sheet_intent = if debug::has_focus() {
            Intent::default()
        } else {
            intent
        };

        let layout = self.layout(ui, sheet_intent);
        self.draw_sheet(ui, &layout, sheet_intent);
        debug::windows(ui, intent);
    }

    fn layout(&mut self, ui: &Ui, intent: Intent) -> Layout {
        let [width, height] = ui.io().display_size;
        let scale = settings::get().ui_scale.max(0.5);
        let style = ui.clone_style();

        let inset = style.window_padding[0];
        let inner_width = (width - inset * 2.0).max(320.0);

        // The chrome comes out of the screen first; the body gets what is left. The top half
        // is measured from the last frame, because the tab strip leaves a gap of its own that
        // a predicted total misses.
        let sheet_padding = style.window_padding[1];
        let item_gap = style.item_spacing[1];
        let footer_height = (ui.text_line_height() + 8.0).round();
        let above = if self.chrome_above > 0.0 {
            self.chrome_above
        } else {
            sheet_padding + chrome::title_height(ui) + item_gap + theme::tabs_height(ui) + item_gap
        };
        let below = item_gap + footer_height + sheet_padding;
        let chrome_height = above + below;
        let available_height = (height - inset - theme::PAD).max(240.0);
        let max_body = (available_height - chrome_height).max(200.0);

        let tabs = tabs();
        let names: Vec<&'static str> = tabs.iter().map(|(label, _)| *label).collect();
        if let Some(step) = intent.tab {
            // The strip wraps: the page after the last one is the first one.
            self.tab = (self.tab as i32 + step).rem_euclid(names.len() as i32) as usize;
            // The row cursor belongs to the page that is up, not to the sheet.
            self.nav_row = None;
        }
        if self.tab >= names.len() {
            self.tab = 0;
        }
        let page = names[self.tab];

        // The sidebar is the detail view of one script, and it has to leave the table enough
        // width to elide a path in; on a narrow screen it steps out instead.
        let sidebar_width = (302.0 * scale).round();
        let gap = 18.0;
        let show_sidebar = page == strings::tabs::SCRIPTS
            && self.scripts.selected.is_some()
            && inner_width - sidebar_width - gap >= 560.0 * scale;

        // Toggled before the sheet is sized: the sidebar's fields list is what makes it taller.
        if intent.fields && show_sidebar {
            self.scripts.fields_open = !self.scripts.fields_open;
        }

        let sidebar_height =
            (if self.scripts.fields_open { 540.0 } else { 380.0 } + ui.text_line_height()) * scale;
        let body_height = match page {
            strings::tabs::LOGS | strings::tabs::LIBRARIES | strings::tabs::AUDIO => max_body,
            strings::tabs::SETTINGS => pages::settings::body_height(ui).min(max_body),
            _ => {
                // One row is one frame tall, plus the header row and the toolbar.
                let rows = ((threads::count() as f32) + 2.5) * ui.frame_height() + 36.0;
                let floor = if show_sidebar { sidebar_height } else { 220.0 };
                rows.max(floor).min(max_body)
            }
        };
        let main_width = if show_sidebar {
            inner_width - sidebar_width - gap
        } else {
            inner_width
        };
        let page_rows = (body_height / ui.frame_height() - 2.0).max(1.0) as i32;

        let down = ui.io().mouse_down[0];
        let click = down && !self.mouse_down;
        self.mouse_down = down;

        Layout {
            size: [width, height],
            inner_width,
            inset,
            tabs,
            names,
            body_height,
            main_width,
            sidebar_width: show_sidebar.then_some(sidebar_width),
            gap,
            page_rows,
            footer_height,
            chrome_height,
            click,
        }
    }

    fn draw_sheet(&mut self, ui: &Ui, layout: &Layout, intent: Intent) {
        ui.window(format!(
            "CLEO {} Menu",
            ptr_to_rust_str(unsafe { CLEO_GetVersionStr() })
        ))
        .menu_bar(false)
        .title_bar(false)
        .size(
            [layout.size[0], layout.chrome_height + layout.body_height + self.sheet_slack],
            Condition::Always,
        )
        // The window cannot be moved, so a remembered position only means ending up off
        // screen after a resolution change.
        .position([0.0, 0.0], Condition::Always)
        .resizable(false)
        .movable(false)
        // The size is computed for the children; a bar or a wheel here means a layout bug.
        .scroll_bar(false)
        .scrollable(false)
        .build(|| {
            chrome::title_bar(ui, layout.inner_width, layout.click);

            let before = self.tab;
            theme::tabs(ui, "main_tabs", &layout.tabs, &mut self.tab);
            if self.tab != before {
                self.nav_row = None;
            }
            let page = layout.names[self.tab];

            // What the title and the strip actually took, for the next frame's height.
            self.chrome_above = ui.cursor_screen_pos()[1] - ui.window_pos()[1];

            self.body(ui, layout, intent, page);
            self.info_panel(ui, layout, page);
            self.footer(ui, layout, page);

            // What the children took, against what the window was given. The difference is
            // added to the next frame's height rather than assigned: the window already
            // carries the last correction, so assigning makes the footer jump.
            let padding = ui.clone_style().window_padding[1];
            let used = ui.cursor_pos()[1] + padding;
            let delta = used - ui.window_size()[1];
            if delta.abs() < 120.0 && delta.abs() > 0.25 {
                self.sheet_slack = (self.sheet_slack + delta).clamp(-80.0, 160.0);
            }
        });
    }

    fn body(&mut self, ui: &Ui, layout: &Layout, intent: Intent, page: &'static str) {
        ui.child_window("main")
            .size([layout.main_width, layout.body_height])
            .build(|| {
                let report = match page {
                    strings::tabs::SCRIPTS => pages::scripts::page(
                        ui,
                        &mut self.scripts,
                        intent,
                        layout.page_rows,
                        layout.main_width,
                    ),
                    strings::tabs::LIBRARIES => pages::libraries::page(
                        ui,
                        &mut self.libraries,
                        &mut self.nav_row,
                        intent,
                        layout.page_rows,
                    ),
                    strings::tabs::AUDIO => {
                        let mut report = Report::default();
                        theme::padded_child(ui, "audio_tab_inner", || {
                            report.shown = pages::audio::page(
                                ui,
                                &mut self.nav_row,
                                intent.row,
                                layout.page_rows,
                            );
                        });
                        report
                    }
                    strings::tabs::LOGS => {
                        self.logs.page(ui, intent, layout.page_rows);
                        Report::default()
                    }
                    strings::tabs::SETTINGS => {
                        pages::settings::page(ui);
                        Report::default()
                    }
                    _ => Report::default(),
                };

                self.rows_shown = report.shown;
                if let Some((key, name, kind)) = report.open {
                    debug::open(&key, &name, kind);
                }
            });
    }

    fn info_panel(&mut self, ui: &Ui, layout: &Layout, page: &'static str) {
        let Some(width) = layout.sidebar_width.filter(|_| page == strings::tabs::SCRIPTS) else {
            return;
        };

        // `SameLine` counts from the window's left edge, not from where the content starts,
        // so the sheet's padding has to be added back.
        ui.same_line_with_spacing(layout.inset + layout.main_width + layout.gap, 0.0);
        let _pad = ui.push_style_var(StyleVar::WindowPadding([14.0, 8.0]));

        let mut open = None;
        ui.child_window("sidebar")
            .size([width, layout.body_height])
            .always_use_window_padding(true)
            .build(|| open = pages::scripts::sidebar::panel(ui, &mut self.scripts));
        drop(_pad);

        if let Some((key, name, kind)) = open {
            debug::open(&key, &name, kind);
        }
    }

    fn footer(&mut self, ui: &Ui, layout: &Layout, page: &'static str) {
        let (summary, note, quick, toggle) = self.footer_text(page);

        let mut open_settings = false;
        // The band is one line tall, so it gets the sheet's horizontal padding and nothing
        // vertical.
        let _pad = ui.push_style_var(StyleVar::WindowPadding([layout.inset, 3.0]));
        ui.child_window("footer")
            .size([layout.inner_width, layout.footer_height])
            .always_use_window_padding(true)
            .scroll_bar(false)
            .build(|| {
                open_settings = chrome::footer(
                    ui,
                    summary.as_deref(),
                    note.as_deref(),
                    quick.as_deref(),
                    toggle.as_deref(),
                    &settings::get().toggle_key_label(),
                );
            });
        drop(_pad);

        if open_settings {
            if let Some(index) = layout
                .names
                .iter()
                .position(|name| *name == strings::tabs::SETTINGS)
            {
                self.tab = index;
            }
        }
    }

    /// The four strings the footer may draw: the page summary, the Scripts table's remark
    /// about dropped columns, the keys that pick a filter chip, and the key that parks the
    /// selected script.
    #[allow(clippy::type_complexity)]
    fn footer_text(
        &self,
        page: &'static str,
    ) -> (
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    ) {
        let shown = self.rows_shown;
        let summary = match page {
            strings::tabs::SCRIPTS => {
                Some(strings::footer::shown(shown, "script", "scripts"))
            }
            strings::tabs::LIBRARIES => {
                Some(strings::footer::shown(shown, "library", "libraries"))
            }
            strings::tabs::AUDIO => Some(strings::footer::shown(
                shown,
                "audio stream",
                "audio streams",
            )),
            strings::tabs::LOGS => Some(strings::footer::log_stream(
                &self.logs.active().path().display().to_string(),
            )),
            strings::tabs::SETTINGS => Some(strings::footer::settings_file(
                &settings_file_path().display().to_string(),
            )),
            _ => return (None, None, None, None),
        };

        let note = (page == strings::tabs::SCRIPTS && !self.scripts.hidden_columns.is_empty())
            .then(|| strings::footer::hidden_columns(&self.scripts.hidden_columns.join(", ")));

        let quick = match page {
            strings::tabs::SCRIPTS => Some(strings::footer::quick_filter(pages::scripts::CHIP_COUNT)),
            strings::tabs::LIBRARIES => {
                Some(strings::footer::quick_filter(pages::libraries::CHIP_COUNT))
            }
            _ => None,
        };

        let toggle = (page == strings::tabs::SCRIPTS)
            .then(|| self.scripts.selected.as_deref().and_then(threads::find))
            .flatten()
            .filter(|entry| threads::alive(entry.ptr))
            .map(|entry| {
                let text = if threads::is_suspended(entry.ptr) {
                    strings::footer::PRESS_SPACE_RESUME
                } else {
                    strings::footer::PRESS_SPACE_PAUSE
                };
                text.to_string()
            });

        (summary, note, quick, toggle)
    }
}

#[ctor::ctor]
fn main() {
    use hudhook::hooks::dx9::ImguiDx9Hooks;
    hudhook!(ImguiDx9Hooks, Dashboard::default());
}
