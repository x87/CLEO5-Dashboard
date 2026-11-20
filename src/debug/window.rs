//! The Script Debugger window: the script as a listing with the line it is standing on
//! marked, its 32 locals, the fields of the running thread, and the controls.
//!
//! Everything here reads the thread the panel already validated; the state machine lives in
//! [`super`], and this file only draws it and turns clicks into requests.

use imgui::{Condition, ListClipper, MouseCursor, StyleColor, Ui, WindowFocusedFlags};

use crate::debug::{LocalTypes, Pane, Request, ScriptDebug, SlotType, State};
use crate::icons::Icon;
use crate::nav::{moved_row, Intent, RowMove};
use crate::sdk::*;
use crate::threads::{self, Kind};
use crate::utils::hex_address;
use crate::highlight;
use crate::theme::Role;
use crate::{settings, strings, theme};

/// Room left of the listing text for the current-line marker.
const GUTTER: f32 = 16.0;

/// How many rows the tallest column of the fields card holds: the `State Flags` captions,
/// which are one caption row and the nine flags. `fields` draws them, so the two change
/// together - it is what the card is sized from.
const FIELDS_ROWS: f32 = 10.0;

/// Draw the window. Returns whether it should stay open.
pub(super) fn draw(
    ui: &Ui,
    entry: &mut ScriptDebug,
    state: &mut State,
    ordinal: usize,
    nav: Intent,
) -> (bool, bool) {
    let mut keep_open = true;
    // Whether this window is the one the user is in. The focused window is the one on top -
    // ImGui raises it when it is clicked - which is what `Escape` backs out of first.
    let mut focused = false;
    // The label the title bar shows, and the id that keeps two windows apart: the name can
    // change under it - a thread names itself with `03A4` after it starts - while the key
    // never does, so the id is the key.
    let listed = if crate::threads::alive(entry.ptr as *mut CRunningScript) {
        unsafe { (*(entry.ptr as *mut CRunningScript)).name() }
    } else {
        String::new()
    };
    let shown = if listed.is_empty() {
        entry.name.clone()
    } else {
        listed
    };
    let title = format!(
        "{} \"{}\"##script_debugger_{}",
        strings::debug::WINDOW, shown, entry.key
    );

    let scale = settings::get().ui_scale.max(0.5);
    // Stacked so a second window does not land exactly on the first; from then on the
    // user's own position and size win.
    let cascade = 28.0 * scale * ordinal as f32;
    // The sheet underneath is translucent and a debugger window inherits that alpha, so the
    // table showed through the listing. A floating window paints its own opaque panel.
    let _bg = ui.push_style_color(
        StyleColor::WindowBg,
        theme::with_alpha(theme::panel_color(), 0.98),
    );

    // Never larger than what is left of the screen below where it opens: at a large UI
    // scale the nominal size ran off the bottom, and the fields panel with it.
    let display = ui.io().display_size;
    let origin = [64.0 * scale + cascade, 40.0 * scale + cascade];
    let nominal = [
        (760.0 * scale).min(display[0] - origin[0] - 16.0),
        (690.0 * scale).min(display[1] - origin[1] - 16.0),
    ];

    // Where the window was left in an earlier session wins over the cascade, pulled back
    // inside the screen first: the game may be running at another resolution than last
    // time, or on another monitor.
    let (size, origin) = match settings::debug_window() {
        Some([x, y, width, height]) => (
            [
                width.min(display[0] - 16.0).max(320.0),
                height.min(display[1] - 16.0).max(240.0),
            ],
            // The second and later windows still step away from the first, and from each
            // other, instead of landing on exactly the same rectangle.
            [
                (x + cascade).clamp(-width + 160.0, display[0] - 160.0).max(0.0),
                (y + cascade).clamp(0.0, display[1] - 40.0).max(0.0),
            ],
        ),
        None => (nominal, origin),
    };

    // A window that was asked for comes forward: clicking the Scripts table raises the
    // menu, which would otherwise bury the debugger behind it every time.
    let focus = std::mem::take(&mut entry.focus);

    let mut geometry = None;
    let mut open = true;
    let visible = ui
        .window(&title)
        .size(size, Condition::FirstUseEver)
        .position(origin, Condition::FirstUseEver)
        .focused(focus)
        .resizable(true)
        .collapsible(false)
        // ImGui's own title bar, with the close button it draws for `opened`: dragging, the
        // right-aligned X and the row moving with the window are its code, not ours.
        .opened(&mut open)
        // The panes and the fields card scroll inside themselves; a window that scrolled
        // too would put a second scrollbar beside the first.
        .scrollable(false)
        .build(|| {
            // The window or anything in it: clicking the listing puts the focus on a child,
            // and the debugger is still what the user is working in.
            focused = ui.is_window_focused_with_flags(WindowFocusedFlags::CHILD_WINDOWS);

            // What the keys mean in here, and only while this window is the one on top:
            // the sheet underneath stands down for the frame, so `Tab` is free to walk
            // these two panes the way it walks the sheet's tabs.
            let keys = if focused { nav } else { Intent::default() };
            if keys.tab.is_some_and(|step| step != 0) {
                entry.pane = match entry.pane {
                    Pane::Disassembly => Pane::Trace,
                    Pane::Trace => Pane::Disassembly,
                };
            }
            // Read before the layout below measures the fields card, so the press shows the
            // new state on the frame it happens rather than the next one.
            if keys.fields {
                settings::set_debug_fields(!settings::debug_fields());
            }

            if !refresh(entry) {
                ui.text_colored(Role::Error.rgba(1.0), strings::debug::GONE);
                if theme::flat_button(ui, strings::debug::CLOSE) {
                    keep_open = false;
                }
                return;
            }

            let script = entry.ptr as *mut CRunningScript;
            // `refresh` found a thread a moment ago, but a script can end between then and
            // the click that writes to it: nothing below touches the pointer unasked.
            if !crate::threads::alive(script) {
                ui.text_colored(Role::Error.rgba(1.0), strings::debug::GONE);
                if theme::flat_button(ui, strings::debug::CLOSE) {
                    keep_open = false;
                }
                return;
            }
            suspension_note(ui, entry, script, state);

            // The panes sit in a child of a fixed height so the fields card below them is
            // the same size whichever pane is up.
            let fields_open = settings::debug_fields();
            let avail = ui.content_region_avail()[1];
            // What sits between the panes and the fields card - separator plus disclosure
            // header - measured from the previous frame, since ImGui's own spacing is not
            // worth predicting. The first frame falls back on a count.
            let style = ui.clone_style();
            let tail = if entry.tail > 0.0 {
                entry.tail
            } else {
                style.item_spacing[1] * 2.0 + 1.0 + ui.text_line_height()
            };
            // The card is as tall as its tallest column - a caption and nine rows - plus its
            // padding.
            let fields_content = FIELDS_ROWS * ui.text_line_height()
                + (FIELDS_ROWS - 1.0) * style.item_spacing[1];
            let wanted_fields = fields_content + theme::PAD * 2.0;

            // The pane switch and the action buttons are static: the split handle resizes the
            // panes and nothing else.
            let header_top = ui.cursor_screen_pos()[1];
            pane_switch(ui, entry);
            let actions = toolbar(ui, entry);
            ui.separator();
            let header = ui.cursor_screen_pos()[1] - header_top;

            // Open, the card gets its content height or what the window can spare past the
            // header and a pane worth looking at; closed it asks for nothing.
            let fields_height = if fields_open {
                wanted_fields.min((avail - header - tail - 180.0).max(120.0))
            } else {
                0.0
            };
            let panes_height = (avail - header - tail - fields_height).max(160.0);

            let _clear = ui.push_style_color(StyleColor::ChildBg, [0.0, 0.0, 0.0, 0.0]);
            // Measured outside the columns: inside one, `content_region_avail` is the
            // column's own width, which is what the split is trying to decide.
            let panes_width = ui.content_region_avail()[0];
            ui.child_window("panes")
                .size([panes_width, panes_height])
                .build(|| {
                    let room = ui.content_region_avail();
                    let top = ui.cursor_screen_pos();
                    let left_edge = top[0];

                    // The split is the user's: dragging the handle between the panes sets
                    // it, and it is remembered for the next window.
                    let split = entry.split.clamp(0.25, 0.80);
                    let left = (room[0] * split).clamp(240.0, (room[0] - 200.0).max(240.0));

                    ui.columns(2, "panes_columns", false);
                    ui.set_column_width(0, left);
                    left_pane(ui, entry, script, keys.row, keys);
                    ui.next_column();
                    variables(ui, script, &entry.local_types);
                    ui.columns(1, "panes_end", false);

                    // The handle goes in after the panes so it wins the pointer: a later
                    // item takes the hover from an earlier one.
                    splitter(ui, entry, left_edge, left, top[1], room[1]);
                });

            // Applied once the panes have drawn their lines: clearing before the draw would
            // leave the frame showing what was just thrown away.
            apply_actions(ui, entry, actions);

            let panes_bottom = ui.cursor_screen_pos()[1];
            ui.separator();
            if theme::section_head(
                ui,
                strings::debug::FIELDS,
                Some(Icon::Flag),
                Some(fields_open),
                0.0,
                Some(strings::keys::FIELDS),
                // A caption like the one above it; the caret carries the accent.
                theme::dim_color(),
            ) {
                settings::set_debug_fields(!fields_open);
            }
            theme::hint(ui, strings::debug::TIP_FIELDS);
            // Measured where the card begins, so the next frame can size the panes to leave
            // exactly this much for the two rows below them.
            entry.tail = ui.cursor_screen_pos()[1] - panes_bottom;
            if fields_open {
                theme::card(ui, "fields_card", fields_height.max(60.0), || {
                    fields(ui, script)
                });
            }

            // Read here, while this window is the current one. `window_pos` is meaningless
            // until ImGui has laid the window out, hence the checks on the way out.
            geometry = Some([
                ui.window_pos()[0],
                ui.window_pos()[1],
                ui.window_size()[0],
                ui.window_size()[1],
            ]);
        })
        // `None` means the window was closed: it is not collapsible, so the title bar's X
        // is the only way out.
        .is_some();

    remember_geometry(ui, geometry);
    (keep_open && visible && open, focused)
}

/// Write the window's place to the ini once the mouse is up again: `CLEO_SetConfigInt`
/// rewrites the whole file, so this must not run per frame while a window is dragged.
fn remember_geometry(ui: &Ui, geometry: Option<[f32; 4]>) {
    let Some(rect) = geometry else {
        return;
    };
    if ui.io().mouse_down[0] || !rect.iter().all(|value| value.is_finite()) {
        return;
    }
    if rect[2] < 240.0 || rect[3] < 200.0 {
        return;
    }

    let changed = settings::debug_window().is_none_or(|saved| {
        (0..4).any(|i| (saved[i] - rect[i]).abs() > 0.5)
    });
    if changed {
        settings::set_debug_window(rect);
    }
}

/// Point the entry at the live thread, and rebuild the listing when the script changed.
///
/// A thread `main.scm` started has no base (`BaseIP` is zero, `m_nIp` is absolute), so its
/// listing opens at the current instruction and is rebuilt once the script moves out of it.
fn refresh(entry: &mut ScriptDebug) -> bool {
    // This also refreshes `code_size`, which is what the listing and the trace are bounded
    // by: exact for a CLEO script, the region it occupies for one of the game's.
    if !super::refresh_pointer(entry) {
        return false;
    }

    let script = entry.ptr as *mut CRunningScript;
    let size = entry.code_size;
    let base = threads::code_base(script) as *const u8;
    if base.is_null() || size == 0 {
        entry.listing = None;
        return true;
    }

    let current = unsafe { (*script).m_nIp }.saturating_sub(base as usize) as u32;
    let first = if threads::has_start(script) {
        0
    } else {
        current.min(size as u32)
    };

    let stale = entry.listing_for != (entry.ptr, size, first);
    // A jump backwards, or out past the end of a capped listing, leaves the marker with no
    // line to sit on; the window follows the script rather than showing an empty band.
    let lost = entry
        .listing
        .as_ref()
        .is_some_and(|listing| listing.index_at(current).is_none());

    if stale || lost {
        let bytes = unsafe { std::slice::from_raw_parts(base.add(first as usize), size - first as usize) };
        entry.listing = Some(super::listing::build(
            bytes,
            super::db(),
            super::scm_block(),
            first,
        ));
        entry.listing_for = (entry.ptr, size, first);
    }

    true
}

/// The window's own header row. Returns `false` when the user asked to close the window.
/// The reason a script is parked, when CLEO left one behind.
///
/// The window's own name, its close button and its drag handle are ImGui's now: the title bar
/// carries `[SD] Script Debugger "name"`, and `opened` gives it the X. Nothing here has to
/// agree with the window's position, which is the point.
fn suspension_note(ui: &Ui, entry: &ScriptDebug, script: *mut CRunningScript, state: &State) {
    if unsafe { (*script).m_nWakeTime } != SUSPENDED_WAKE_TIME {
        return;
    }
    let Some(error) = &state.last_error else {
        return;
    };
    let summary = error.lines().next().unwrap_or_default();
    if !entry.name.is_empty() && summary.contains(entry.name.as_str()) {
        ui.text_colored(
            Role::Error.rgba(1.0),
            strings::debug::suspended_reason(summary),
        );
    }
}

fn left_pane(
    ui: &Ui,
    entry: &mut ScriptDebug,
    script: *mut CRunningScript,
    movement: Option<RowMove>,
    keys: Intent,
) {
    // Only the button row is left above and below the list: the pane switch and the action
    // card are the window's own header now, so dragging the split between the panes moves
    // the list and the variables column and leaves the controls alone.
    let rows = ui.frame_height();
    let spacing = ui.clone_style().item_spacing[1];
    let chrome = rows + spacing; // the buttons under the list
    let height = (ui.content_region_avail()[1] - chrome).max(80.0);

    match entry.pane {
        Pane::Disassembly => drawing(ui, entry, script, height, movement),
        Pane::Trace => trace(ui, entry, script, height, movement),
    }

    buttons(ui, entry, script, keys);
}

/// What the action card asked for, applied once the pane has been drawn.
fn apply_actions(ui: &Ui, entry: &mut ScriptDebug, actions: Actions) {
    // `Clear` belongs to the trace alone - it empties the ring - so the pane it was pressed
    // in is the only one that can say what it means.
    if actions.clear {
        entry.ring.clear();
        entry.trace_cursor = None;
    }
    if actions.copy {
        let lines = match entry.pane {
            Pane::Trace => trace_lines(entry),
            Pane::Disassembly => listing_lines(entry),
        };
        ui.set_clipboard_text(lines.join("\n"));
    }
}

/// The drag handle between the listing and the variables: an invisible strip on the boundary,
/// drawn as a rule so there is something to aim at.
fn splitter(ui: &Ui, entry: &mut ScriptDebug, left_edge: f32, left: f32, top: f32, height: f32) {
    let width = 10.0;
    let center = left_edge + left;
    ui.set_cursor_screen_pos([center - width * 0.5, top]);
    ui.invisible_button("##panes_split", [width, height]);

    let hovered = ui.is_item_hovered();
    let active = ui.is_item_active();
    if hovered || active {
        ui.set_mouse_cursor(Some(MouseCursor::ResizeEW));
    }

    if active {
        let room = ui.content_region_avail()[0];
        if room > 1.0 {
            let mouse = ui.io().mouse_pos[0];
            entry.split = ((mouse - left_edge) / room).clamp(0.25, 0.80);
        }
    }
    // Written once the handle is let go: a write per drag frame would rewrite the ini per
    // frame. ImGui reports no deactivation on an invisible button, hence the mouse check.
    if !ui.io().mouse_down[0] && (entry.split - settings::debug_split()).abs() > 0.005 {
        settings::set_debug_split(entry.split);
    }

    // The rule itself: brighter while the pointer is on it, so the split reads as movable.
    let color = if hovered || active {
        theme::with_alpha(theme::accent_color(), 0.85)
    } else {
        theme::with_alpha(theme::dim_color(), 0.55)
    };
    let draw = ui.get_window_draw_list();
    draw.add_line([center, top], [center, top + height], color)
    .thickness(if hovered || active { 2.0 } else { 1.0 })
    .build();
}

/// The `Disassembly` / `Trace` switch, drawn as the same strip the sheet's tabs use: the
/// two panes are what this window has instead of pages.
fn pane_switch(ui: &Ui, entry: &mut ScriptDebug) {
    let items = [
        (strings::debug::PANE_DISASM, Icon::Journal),
        (strings::debug::PANE_TRACE, Icon::Play),
    ];
    let mut selected = match entry.pane {
        Pane::Disassembly => 0,
        Pane::Trace => 1,
    };

    let clicked = theme::tabs(ui, "debug_panes", &items, &mut selected);
    entry.pane = if clicked == 0 {
        Pane::Disassembly
    } else {
        Pane::Trace
    };
}

/// What the toolbar's buttons asked for.
#[derive(Default)]
struct Actions {
    clear: bool,
    copy: bool,
}

/// The controls both panes share: follow the newest line, clear, copy, and the line form.
fn toolbar(ui: &Ui, entry: &mut ScriptDebug) -> Actions {
    let mut actions = Actions::default();
    let mut more = false;

    theme::card_fixed(ui, "pane_toolbar", ui.frame_height() + 8.0, || {
        // Switching following back on is a decision to watch the script: the line the keys
        // were on is dropped, and the pane centres on the script's own line at once.
        let was_following = entry.auto_scroll;
        theme::switch(ui, "##pane_auto_scroll", &mut entry.auto_scroll);
        if entry.auto_scroll && !was_following {
            entry.trace_cursor = None;
            entry.disasm_cursor = None;
        }
        theme::hint(ui, strings::debug::TIP_AUTO_SCROLL);
        ui.same_line();
        ui.align_text_to_frame_padding();
        // The switch is the control; the word beside it is its caption.
        ui.text_colored(theme::dim_color().to_rgba_f32s(), strings::debug::AUTO_SCROLL);

        // The three actions are icons: in a pane this narrow the words alone pushed Copy
        // and More off the edge of the toolbar, and More is where the line cap lives.
        let action = (ui.text_line_height() * 1.05).round();
        ui.same_line_with_spacing(0.0, 14.0);
        // `Clear` empties the trace ring. The disassembly is a listing of the script: no
        // click could empty it, so the button is not offered there at all rather than being
        // there and doing nothing.
        if entry.pane == Pane::Trace {
            if theme::icon_button(
                ui,
                "##pane_clear",
                crate::icons::Icon::Trash,
                action,
                // Buttons: accent, the way every control in the menu is.
                theme::accent_color(),
            ) {
                actions.clear = true;
            }
            theme::hint(ui, strings::debug::TIP_CLEAR);
            ui.same_line();
        }

        if theme::icon_button(
            ui,
            "##pane_copy",
            crate::icons::Icon::Clipboard,
            action,
            theme::accent_color(),
        ) {
            actions.copy = true;
        }
        theme::hint(ui, strings::debug::TIP_COPY);

        ui.same_line();
        if theme::icon_button(
            ui,
            "##pane_more",
            crate::icons::Icon::Sliders,
            action,
            theme::accent_color(),
        ) {
            more = true;
        }
        theme::hint(ui, strings::debug::TIP_MORE);
    });

    if more {
        ui.open_popup("pane_more");
    }
    let mut compact = settings::debug_compact();
    if let Some(_popup) = ui.begin_popup("pane_more") {
        if theme::switch(ui, "##pane_compact", &mut compact) {
            settings::set_debug_compact(compact);
        }
        ui.same_line();
        // Caption of the switch above it, so it is dimmed.
        ui.text_colored(theme::dim_color().to_rgba_f32s(), strings::debug::COMPACT);
        theme::hint(ui, strings::debug::TIP_COMPACT);
    }

    actions
}

/// The lines the trace pane shows, in the form the toolbar asks for.
fn trace_lines(entry: &ScriptDebug) -> Vec<String> {
    let compact = settings::debug_compact();
    entry
        .ring
        .iter()
        .map(|line| {
            if compact {
                line.compact.clone()
            } else {
                line.text.clone()
            }
        })
        .collect()
}

/// The whole listing as text, one line per instruction, the way the pane draws it.
fn listing_lines(entry: &ScriptDebug) -> Vec<String> {
    let Some(listing) = entry.listing.as_ref() else {
        return Vec::new();
    };
    let compact = settings::debug_compact();
    listing
        .entries
        .iter()
        .map(|line| {
            let body = if compact { &line.compact } else { &line.text };
            format!("{:04}: {}", line.offset, body)
        })
        .collect()
}

// --------------------------------------------------------------------- listing

fn drawing(
    ui: &Ui,
    entry: &mut ScriptDebug,
    script: *mut CRunningScript,
    height: f32,
    movement: Option<RowMove>,
) {
    let base = threads::code_base(script);
    let current = unsafe { (*script).m_nIp };
    let offset = current.saturating_sub(base) as u32;

    let Some(listing) = entry.listing.as_ref() else {
        ui.text_colored(Role::Dim.rgba(1.0), strings::debug::LISTING_EMPTY);
        return;
    };

    let highlight = listing.index_at(offset);
    let compact = settings::debug_compact();
    let auto_scroll = entry.auto_scroll;
    let mut unfollow = false;

    // The keyboard's line. A first press starts on the line the script is standing on, so
    // the cursor is where the eye already is.
    let len = listing.len();
    if entry.disasm_cursor.is_some_and(|row| row >= len) {
        // The listing was rebuilt under the cursor.
        entry.disasm_cursor = None;
    }
    if let Some(movement) = movement {
        let start = entry.disasm_cursor.or(highlight);
        let page = (height / ui.text_line_height()).max(1.0) as i32;
        entry.disasm_cursor = moved_row(start, Some(movement), len, page);
        // Walking the listing is a decision to read it, so the current line stops pulling
        // the view around.
        if entry.disasm_cursor.is_some_and(|row| Some(row) != highlight) {
            unfollow = true;
        }
    }
    let picked = entry.disasm_cursor.filter(|row| *row < len);

    theme::card_wide(ui, "listing", height, || {
        let Some(_mono) = theme::mono_font(ui) else {
            return;
        };

        let line_height = ui.text_line_height();
        let pitch = line_height + ui.clone_style().item_spacing[1];
        // The number column is as wide as its widest number, plus a gap wide enough to
        // separate the two - at the default 10 px the offset and the mnemonic ran together
        // into one word. The gap follows the font, so it holds at any UI scale.
        let text_left =
            GUTTER + ui.calc_text_size("0000:")[0] + (line_height * 1.1).round();
        // How wide the pane has to be for the widest line, so that a long string literal
        // can be reached by scrolling rather than being cut off at the edge. The font is
        // monospaced, so the longest line's character count times one advance is enough.
        let advance = ui.calc_text_size("0")[0];
        let content = text_left + (listing.widest as f32 + 1.0) * advance;
        let width = content.max(ui.content_region_avail()[0]);

        let mut clipper = ListClipper::new(listing.len() as i32)
            .items_height(pitch)
            .begin(ui);
        while clipper.step() {
            for index in clipper.display_start()..clipper.display_end() {
                let line = &listing.entries[index as usize];
                let current = index as usize == highlight.unwrap_or(usize::MAX);
                let selected = picked == Some(index as usize);
                row(ui, line, current, selected, compact, width, text_left);
            }
        }

        // Auto-scroll keeps the marked line centred. A wheel movement in either direction
        // turns following off; walking the list with the keys does too (handled above).
        // The scroll is set from the line's own offset: `set_scroll_here_y` aims at the
        // cursor, which by now is past the list.
        if theme::wheel_scrolled(ui) {
            unfollow = true;
        } else if movement.is_none() && auto_scroll {
            if let Some(index) = highlight {
                let target = index as f32 * pitch - (ui.window_size()[1] - pitch) * 0.5;
                ui.set_scroll_y(target.max(0.0));
            }
        }

        // Bring the line the keys moved to just far enough into view. The clipper only draws
        // what is on screen, so the position comes from the row index.
        if let Some(index) = picked {
            if movement.is_some() {
                let line_top = index as f32 * pitch;
                let view_top = ui.scroll_y();
                let view_bottom = view_top + ui.window_size()[1];
                let target = if line_top + pitch > view_bottom {
                    line_top + pitch - ui.window_size()[1]
                } else if line_top < view_top {
                    line_top
                } else {
                    view_top
                };
                ui.set_scroll_y(target.max(0.0));
            }
        }

        // A listing that stopped early says so where it stopped.
        if listing.capped {
            ui.text_colored(
                Role::Warning.rgba(1.0),
                strings::debug::LISTING_CAPPED,
            );
        }
        if let Some(stop) = listing.stop {
            ui.text_colored(Role::Warning.rgba(1.0), stop_text(stop));
        }
    });

    if unfollow {
        entry.auto_scroll = false;
    }
}

/// One listing row: number, marker when it is the line the script is standing on, text.
///
/// `compact` drops the command name and the `{argument}` comments from the body, the way the
/// trace does; the offset column is the same either way. `selected` is the line the keys
/// are on, which is a different thing from the line the script is on - that one is green,
/// the way the trace marks the instruction that has not run yet.
fn row(
    ui: &Ui,
    line: &crate::debug::listing::Entry,
    current: bool,
    selected: bool,
    compact: bool,
    width: f32,
    text_left: f32,
) {
    let line_height = ui.text_line_height();
    let top = ui.cursor_screen_pos();
    let draw = ui.get_window_draw_list();

    if selected {
        draw.add_rect(
            [top[0] - 4.0, top[1] - 1.0],
            [top[0] + width + 4.0, top[1] + line_height + 1.0],
            theme::row_selected_color(),
        )
        .filled(true)
        .build();
    }

    if current {
        // A band across the row and the gutter marker: green, the colour the trace gives
        // the instruction that is about to run.
        let green = Role::Success.color();
        draw.add_rect_filled_multicolor(
            [top[0] - 4.0, top[1] - 1.0],
            [top[0] + width + 4.0, top[1] + line_height + 1.0],
            theme::with_alpha(green, 0.22),
            theme::with_alpha(green, 0.22),
            theme::with_alpha(green, 0.10),
            theme::with_alpha(green, 0.10),
        );

        let cx = top[0] + 3.0;
        let cy = top[1] + line_height * 0.5;
        let _ = draw
            .add_triangle([cx, cy - 5.0], [cx, cy + 5.0], [cx + 8.0, cy], green)
            .filled(true);
    }

    let number = format!("{:04}:", line.offset);
    draw.add_text(
        [top[0] + GUTTER, top[1]],
        Role::Dim.color(),
        number.as_str(),
    );
    draw.add_text(
        [top[0] + text_left, top[1]],
        if current {
            Role::Success.color()
        } else {
            Role::Text.color()
        },
        if compact {
            line.compact.as_str()
        } else {
            line.text.as_str()
        },
    );

    // Advance the row. The draw list painted it, so the item is only a spacer.
    ui.dummy([width, line_height]);
}

fn stop_text(stop: crate::debug::decode::Stop) -> String {
    match stop {
        crate::debug::decode::Stop::Unknown(byte) => {
            format!("{} (type 0x{byte:02X})", strings::debug::LISTING_BROKEN)
        }
        crate::debug::decode::Stop::Truncated => {
            format!("{} (out of bytes)", strings::debug::LISTING_BROKEN)
        }
        crate::debug::decode::Stop::Done | crate::debug::decode::Stop::ArgsEnd => {
            strings::debug::LISTING_BROKEN.to_string()
        }
    }
}

// ----------------------------------------------------------------------- trace

fn trace(
    ui: &Ui,
    entry: &mut ScriptDebug,
    script: *mut CRunningScript,
    height: f32,
    movement: Option<RowMove>,
) {
    // Collected before the pane: the toolbar decides which form the lines take.
    let shown = trace_lines(entry);
    // The instruction about to run, drawn after the recorded ones: the ring only holds what
    // has already run.
    let compact = settings::debug_compact();
    let pending = super::upcoming(entry, script).map(|line| {
        if compact {
            line.compact
        } else {
            line.text
        }
    });
    let auto_scroll = entry.auto_scroll;
    let mut unfollow = false;
    let dropped = entry.ring.dropped();

    // The keyboard's line, counted from the newest recorded one so that a line the script
    // runs afterwards arriving at the end does not drag the cursor off the line it is on.
    // The newest line is where a first press starts, and `Down` from there stays put.
    let len = shown.len();
    if entry.trace_cursor.is_some_and(|above| above >= len) {
        // The line fell out of the ring under it.
        entry.trace_cursor = None;
    }
    if let Some(movement) = movement {
        let above = entry.trace_cursor.unwrap_or(0);
        let start = len.checked_sub(1).map(|last| last - above);
        let page = (height / ui.text_line_height()).max(1.0) as i32;
        let row = moved_row(start, Some(movement), len, page);
        entry.trace_cursor = row.map(|row| len - 1 - row);
        // Reading history means the newest line stops pulling the view to the bottom.
        if entry.trace_cursor.is_some_and(|above| above > 0) {
            unfollow = true;
        }
    }
    let picked = entry.trace_cursor.filter(|above| *above < len);

    theme::card(ui, "trace_lines", height, || {
        let Some(_mono) = theme::mono_font(ui) else {
            return;
        };

        if shown.is_empty() && pending.is_none() {
            ui.text_colored(
                Role::Dim.rgba(1.0),
                strings::debug::EMPTY,
            );
            return;
        }

        if dropped > 0 {
            ui.text_colored(
                Role::Dim.rgba(1.0),
                strings::debug::earlier(dropped),
            );
        }

        // The lines are painted the way the Logs tab paints its own: offset, indentation and
        // opcode are structure and read dim, the instruction is the value.
        let colors = highlight::Colors::theme();
        let mut painter = highlight::Painter::default();
        let syntax = settings::highlight();
        for (index, text) in shown.iter().enumerate() {
            let top = ui.cursor_screen_pos()[1];
            let selected = picked == Some(len - 1 - index);
            if selected {
                // The band goes in first: the text is added to the same list right after
                // it, so the draw order puts it on top.
                let left = ui.window_pos()[0];
                let right = left + ui.window_size()[0];
                let band = ui.text_line_height() + ui.clone_style().item_spacing[1];
                ui.get_window_draw_list()
                    .add_rect([left, top], [right, top + band], theme::row_selected_color())
                    .filled(true)
                    .build();
            }
            painter.draw(ui, text, None, &colors, syntax);

            // Only a key scrolls the list: a line the cursor is merely left on stays where
            // the wheel put it.
            if selected && movement.is_some() {
                theme::reveal_y(ui, top, ui.text_line_height(), ui.window_pos()[1]);
            }
        }
        if let Some(text) = &pending {
            ui.text_colored(Role::Success.rgba(1.0), text);
        }

        if auto_scroll && !unfollow {
            if theme::wheel_scrolled(ui) {
                unfollow = true;
            } else {
                ui.set_scroll_here_y_with_ratio(1.0);
            }
        }
    });

    if unfollow {
        entry.auto_scroll = false;
    }
}

/// Whether Pause and Step have to keep their hands off this script: parking a mission
/// script mid-mission leaves the mission state inconsistent.
fn mission_locked(script: *mut CRunningScript) -> bool {
    let mission = unsafe { (*script).m_bIsMission };
    mission && !settings::allow_native_control()
}

// -------------------------------------------------------------------- controls

/// Follow the script again: the panes recentre on its line and the keyboard's line goes away.
/// Resuming or stepping is a decision to watch the script, not to keep reading where it was.
fn follow(entry: &mut ScriptDebug) {
    entry.auto_scroll = true;
    entry.trace_cursor = None;
    entry.disasm_cursor = None;
}

/// Park the script between two instructions, keeping the time it was due to run again.
///
/// The write is what parks it: the engine skips a thread whose wake time is in the future.
/// It only happens on a pointer CLEO still owns - a script that ended leaves its address
/// behind, and a write through it is a write into whatever took the memory.
fn park(entry: &mut ScriptDebug, script: *mut CRunningScript) {
    let wake = unsafe { (*script).m_nWakeTime };
    entry.control.request = Some(Request::Pause);
    entry.control.paused = true;
    entry.control.remember_wake(wake);
    if crate::threads::alive(script) {
        unsafe { (*script).m_nWakeTime = SUSPENDED_WAKE_TIME };
    }
}

/// Let the parked script run again, on the clock it was parked with.
fn release(entry: &mut ScriptDebug, script: *mut CRunningScript) {
    entry.control.request = Some(Request::Resume);
    entry.control.paused = false;
    let wake = entry.control.saved_wake.take().unwrap_or(0);
    follow(entry);
    if crate::threads::alive(script) {
        unsafe { (*script).m_nWakeTime = wake };
    }
}

/// Run one instruction, then park again.
///
/// A script sitting on a long `wait` is woken for that one instruction, which is why the
/// wake time is set here rather than left to the callback.
fn step(entry: &mut ScriptDebug, script: *mut CRunningScript) {
    entry.control.request = Some(Request::Step(1));
    entry.control.paused = true;
    entry.control.budget = 1;
    entry.control.saved_wake = None;
    follow(entry);
    if crate::threads::alive(script) {
        unsafe { (*script).m_nWakeTime = 0 };
    }
}

/// A key drawn muted after a button: the same action without the mouse, outside the button's
/// hit target.
fn key_label(ui: &Ui, label: &str) {
    ui.align_text_to_frame_padding();
    ui.text_colored(theme::dim_color().to_rgba_f32s(), label);
}

fn buttons(ui: &Ui, entry: &mut ScriptDebug, script: *mut CRunningScript, keys: Intent) {
    let wake = unsafe { (*script).m_nWakeTime };
    // Parked counts both ways: this window parked it, or the Scripts table, or CLEO after an
    // error. The way out is the same, so the button is live either way.
    let parked = entry.control.paused || wake == SUSPENDED_WAKE_TIME;
    let blocked = mission_locked(script);

    // The keys stand down wherever the buttons do.
    if !blocked {
        if keys.pause {
            if parked {
                release(entry, script);
            } else {
                park(entry, script);
            }
        }
        if keys.step {
            step(entry, script);
        }
    }

    // Read again: a key above has already changed the state, and the label is drawn now.
    let parked = entry.control.paused || unsafe { (*script).m_nWakeTime } == SUSPENDED_WAKE_TIME;

    // One button for both states: the label says what pressing it does, the icon its shape.
    let (icon, label) = if parked {
        (crate::icons::Icon::Play, strings::debug::RESUME)
    } else {
        (crate::icons::Icon::Pause, strings::debug::PAUSE)
    };
    // A greyed control that says why is a locked door; one that says nothing is a bug report.
    let tip = if blocked {
        strings::debug::TIP_LOCKED
    } else if parked {
        strings::debug::TIP_RESUME
    } else {
        strings::debug::TIP_PAUSE
    };
    {
        let _disabled = ui.begin_disabled(blocked);
        if theme::flat_button_icon(ui, icon, label) {
            if parked {
                release(entry, script);
            } else {
                park(entry, script);
            }
        }
    }
    theme::hint(ui, tip);
    ui.same_line_with_spacing(0.0, 6.0);
    key_label(ui, strings::keys::PAUSE);
    ui.same_line_with_spacing(0.0, 14.0);

    // Step works from any state, including a script parked by somebody else: walking one is
    // how it is read, not how it is started.
    {
        let _disabled = ui.begin_disabled(blocked);
        if theme::flat_button_icon(ui, crate::icons::Icon::ArrowDown, strings::debug::STEP) {
            step(entry, script);
        }
    }
    theme::hint(ui, if blocked { strings::debug::TIP_LOCKED } else { strings::debug::TIP_STEP });
    ui.same_line_with_spacing(0.0, 6.0);
    key_label(ui, strings::keys::STEP);

    // How much has been recorded, on the right of the same row. The button above already
    // says whether the script is parked, so a lamp saying the same thing is noise.
    //
    // The count stops a gutter short of the line that splits the panes rather than ending on
    // it: the rule is the edge of this pane, and the text beside it keeps the same kind of
    // distance the table's cells keep from the rule between two columns.
    let right = ui.content_region_max()[0] - crate::theme::PAD;
    // Recorded against the cap, `40/2000`, on the trace: how much of the history is in hand,
    // and how much of it the ring keeps. The disassembly counts its own lines instead - it
    // is the script, fixed text, and the ring's fill would be a number about something else.
    let (text, tip) = match entry.pane {
        Pane::Trace => (
            strings::debug::line_count(entry.ring.len(), entry.ring.capacity()),
            strings::debug::TIP_LINE_COUNT,
        ),
        Pane::Disassembly => (
            strings::debug::listing_count(
                entry.listing.as_ref().map_or(0, |listing| listing.len()),
            ),
            strings::debug::TIP_LISTING_COUNT,
        ),
    };
    let width = ui.calc_text_size(&text)[0];
    if right - width > ui.cursor_pos()[0] + 16.0 {
        theme::same_line_at(ui, right - width);
        ui.align_text_to_frame_padding();
        // A reading, so plain text.
        ui.text_colored(Role::Text.rgba(1.0), text);
        theme::hint(ui, tip);
    }
}

// ------------------------------------------------------------------- variables

fn variables(ui: &Ui, script: *mut CRunningScript, types: &LocalTypes) {
    let locals = unsafe { (*script).m_anLocalVariables };
    let timer_a = unsafe { (*script).m_nTimerA };
    let timer_b = unsafe { (*script).m_nTimerB };

    let height = ui.content_region_avail()[1].max(60.0);
    theme::card(ui, "variables_card", height, || {
        // Only the grid scrolls: the header and the timers stay put, since the timers are
        // wanted most when the pane is short.
        theme::section(ui, &strings::debug::variables(locals.len()));

        let style = ui.clone_style();
        // The separator and the timer line, measured as ImGui spends them: a missing gap
        // leaves the grid a few pixels short, which is enough for a scrollbar.
        let timers_height = ui.text_line_height() + style.item_spacing[1] * 2.0 + 1.0;
        let grid_height = (ui.content_region_avail()[1] - timers_height).max(40.0);

        let _clear = ui.push_style_color(StyleColor::ChildBg, [0.0, 0.0, 0.0, 0.0]);
        ui.child_window("variables_grid")
            .size([ui.content_region_avail()[0], grid_height])
            .border(false)
            .build(|| {
                variables_grid(ui, &locals, types);
            });

        ui.separator();
        if let Some(_mono) = theme::mono_font(ui) {
            timer(ui, strings::debug::TIMER_A, timer_a);
            ui.same_line_with_spacing(0.0, 24.0);
            timer(ui, strings::debug::TIMER_B, timer_b);
        }
    });
}

/// `TIMERA = 1234`: the name is a label and the number is the value.
fn timer(ui: &Ui, name: &str, value: i32) {
    ui.text_colored(
        theme::dim_color().to_rgba_f32s(),
        strings::debug::timer_label(name),
    );
    ui.same_line_with_spacing(0.0, 0.0);
    ui.text_colored(
        Role::Text.rgba(1.0),
        strings::debug::timer_value(value),
    );
}

/// The 32 locals, sixteen rows of two - or thirty-two of one when the grid is too narrow
/// to hold two without their running into each other.
fn variables_grid(ui: &Ui, locals: &[i32; 32], types: &LocalTypes) {
    let start = ui.cursor_pos()[0];
    // The grid's own width, which is what decides whether two columns fit - not the pane's.
    let room = ui.content_region_avail()[0];
    let half = room * 0.5;

    // The second cell is placed at the halfway mark, so two columns fit only while the widest
    // of the 32 cells still leaves a gap in the middle. The cell is drawn in two pieces and
    // measured in two.
    let widest = (0..locals.len())
        .map(|cell| {
            ui.calc_text_size(strings::debug::local_label(cell))[0]
                + ui.calc_text_size(local_value_text(locals[cell], types.get(cell)))[0]
        })
        .fold(0.0, f32::max);
    let two = room >= widest * 2.0 + COLUMN_GAP;
    let rows = if two { 16 } else { 32 };
    let draw = |cell: usize| {
        // `[ 7] = 500`: the slot is the label, its contents are the value.
        ui.text_colored(
            theme::dim_color().to_rgba_f32s(),
            strings::debug::local_label(cell),
        );
        ui.same_line_with_spacing(0.0, 0.0);
        ui.text_colored(
            Role::Text.rgba(1.0),
            local_value_text(locals[cell], types.get(cell)),
        );
        theme::hint(ui, local_hint(locals[cell], types.get(cell)));
    };

    for row in 0..rows {
        draw(row);
        if two {
            theme::same_line_at(ui, start + half);
            draw(row + rows);
        }
    }
}

/// The gap the two columns have to have between them before the grid keeps both.
///
/// The second column starts at exactly half the grid's width, so the space either side of
/// that mark is `room / 2 - widest`: requiring `room >= 2 * widest + COLUMN_GAP` is asking
/// for `COLUMN_GAP / 2` of clear space on each side. At 16 that was eight pixels - enough
/// for the columns not to touch and no more, so a grid a hair over the minimum already
/// read as one run of text. Half a mono character either side is what makes two columns
/// look deliberate, and the cells are not fixed width, so a narrow grid collapsing to one
/// column costs nothing but height.
const COLUMN_GAP: f32 = 48.0;

/// A local's value, as the grid shows it: what the trace learned about the slot first, and
/// otherwise plain when it reads like a number, spaced hex when it reads like an address.
fn local_value_text(bits: i32, kind: SlotType) -> String {
    match kind {
        // A declared type is the script's own answer, so the bits are read that way and
        // nothing is guessed on top of it.
        SlotType::Float => return crate::debug::render::format_float(f32::from_bits(bits as u32)),
        // Not a float. Size still says whether it reads as an address, and an address is
        // read as hex - a pointer printed in decimal helps nobody.
        SlotType::Int => return number_text(bits),
        SlotType::Unknown => {}
    }

    if bits == 0 {
        return "0".to_string();
    }

    let as_float = f32::from_bits(bits as u32);
    let floaty = as_float.is_finite()
        && as_float != 0.0
        && as_float.abs() < 1.0e7
        && (as_float.fract() != 0.0 || as_float.abs() < 1.0e-3);

    if floaty {
        crate::debug::render::format_float(as_float)
    } else {
        number_text(bits)
    }
}

/// An integer slot: plain when it reads like a number, spaced hex when it reads like an
/// address.
fn number_text(bits: i32) -> String {
    if bits == 0 {
        return "0".to_string();
    }

    if bits.unsigned_abs() <= 1_000_000 {
        bits.to_string()
    } else {
        format!("0x{:08X}", bits as u32)
    }
}

/// What the value most likely is, for the tooltip. A slot holds 4 bytes and nothing records
/// what wrote them, so unless the trace saw it used the hint is a guess.
fn local_hint(bits: i32, kind: SlotType) -> &'static str {
    match kind {
        SlotType::Float => return strings::debug::HINT_TYPED_FLOAT,
        SlotType::Int => return strings::debug::HINT_TYPED_INT,
        SlotType::Unknown => {}
    }

    if bits == 0 {
        return strings::debug::HINT_ZERO;
    }

    let as_float = f32::from_bits(bits as u32);
    let floaty = as_float.is_finite()
        && as_float != 0.0
        && as_float.abs() < 1.0e7
        && (as_float.fract() != 0.0 || as_float.abs() < 1.0e-3);

    if floaty {
        strings::debug::HINT_FLOAT
    } else if bits.unsigned_abs() <= 1_000_000 {
        strings::debug::HINT_INT
    } else {
        strings::debug::HINT_POINTER
    }
}

// ---------------------------------------------------------------- fields/flags

fn fields(ui: &Ui, script: *mut CRunningScript) {
    let name = unsafe { (*script).name() };
    let next = unsafe { (*script).m_pNext };
    let previous = unsafe { (*script).m_pPrev };
    let base = unsafe { (*script).m_nBaseIp };
    let current = unsafe { (*script).m_nIp };
    let stack_pointer = unsafe { (*script).m_nStackPointer };
    let wake = unsafe { (*script).m_nWakeTime };

    // Caption, the engine field it comes from, and the value. Nine flags, which is what
    // `FIELDS_ROWS` sizes the card for.
    let flags: [(&str, &str, bool); 9] = [
        (
            strings::debug::FLAG_ACTIVE,
            strings::debug::TIP_FLAG_ACTIVE,
            unsafe { (*script).m_bIsActive },
        ),
        (
            strings::debug::FLAG_COND,
            strings::debug::TIP_FLAG_COND,
            unsafe { (*script).m_bCondResult },
        ),
        (
            strings::debug::FLAG_CLEANUP,
            strings::debug::TIP_FLAG_CLEANUP,
            unsafe { (*script).m_bUseMissionCleanup },
        ),
        (
            strings::debug::FLAG_EXTERNAL,
            strings::debug::TIP_FLAG_EXTERNAL,
            unsafe { (*script).m_bIsExternal },
        ),
        (
            strings::debug::FLAG_TEXT_OVERRIDE,
            strings::debug::TIP_FLAG_TEXT_OVERRIDE,
            unsafe { (*script).m_bTextBlockOverride },
        ),
        (
            strings::debug::FLAG_MISSION,
            strings::debug::TIP_FLAG_MISSION,
            unsafe { (*script).m_bIsMission },
        ),
        (
            strings::debug::FLAG_CUSTOM,
            strings::debug::TIP_FLAG_CUSTOM,
            unsafe { (*script).is_custom() },
        ),
        (
            strings::debug::FLAG_WASTED_CHECK,
            strings::debug::TIP_FLAG_WASTED_CHECK,
            unsafe { (*script).m_bWastedBustedCheck },
        ),
        (
            strings::debug::FLAG_WASTED,
            strings::debug::TIP_FLAG_WASTED,
            unsafe { (*script).m_bWastedOrBusted },
        ),
    ];

    let external_type = unsafe { (*script).m_nExternalType };
    let logical = unsafe { (*script).m_nAndOrState };
    let not_flag = unsafe { (*script).m_bNotFlag };
    let scm_function = unsafe { (*script).scm_function() };

    // The gosub stack has no row of its own; it is what SP counts, so it goes on SP's tip.
    let stack = unsafe { (*script).m_anStack };
    let depth = (stack_pointer as usize).min(stack.len());
    let stack_text = if depth == 0 {
        String::new()
    } else {
        let addresses: Vec<String> = stack[..depth]
            .iter()
            .map(|address| format!("0x{address:08X}"))
            .collect();
        strings::debug::gosub_stack(&addresses.join(" > "))
    };

    // Scripts the panel can jump to: the two neighbours in the engine's thread list.
    let targets = [link_target(next as usize), link_target(previous as usize)];

    ui.columns(3, "fields_columns", true);
    // Columns butt against each other and their rules; this gutter keeps a long caption from
    // starting where the previous column's values end.
    let gutter = (ui.text_line_height() * 0.6).round();

    ui.indent_by(gutter);
    let pairs: [(&str, String, String, Option<usize>); 7] = [
        (strings::debug::FIELD_SCRIPT_NAME, name, String::new(), None),
        (
            strings::debug::FIELD_NEXT,
            hex_address(next as usize),
            String::new(),
            Some(0),
        ),
        (
            strings::debug::FIELD_PREVIOUS,
            hex_address(previous as usize),
            String::new(),
            Some(1),
        ),
        (
            strings::debug::FIELD_BASE_IP,
            hex_address(base),
            String::new(),
            None,
        ),
        (
            strings::debug::FIELD_CURRENT_IP,
            hex_address(current),
            String::new(),
            None,
        ),
        (
            strings::debug::FIELD_SP,
            stack_pointer.to_string(),
            stack_text,
            None,
        ),
        (
            strings::debug::FIELD_WAKE_TIME,
            wake.to_string(),
            String::new(),
            None,
        ),
    ];
    for (label, value, tip, link) in &pairs {
        // A neighbour that is still a live script gets a button that opens it.
        let target = link.and_then(|index| targets[index].as_ref());
        let open = target.map(|_| {
            (
                crate::icons::Icon::External,
                strings::sidebar::TIP_OPEN_POINTER,
            )
        });
        let (_, clicked) = theme::action_pair(ui, label, value, Role::Text.color(), open);
        if target.is_some() {
            theme::hint(ui, strings::sidebar::TIP_OPEN_POINTER);
        } else if !tip.is_empty() {
            theme::hint(ui, tip);
        }
        if clicked {
            if let Some((key, name, kind)) = target {
                // Queued, not opened here: this window is drawn while the panel holds the
                // state lock, and `open` takes it again.
                super::request_open(key, name, *kind);
            }
        }
    }

    ui.unindent_by(gutter);
    ui.next_column();
    ui.indent_by(gutter);
    ui.text_colored(
        Role::Dim.rgba(1.0),
        strings::debug::STATE_FLAGS,
    );
    theme::hint(ui, strings::debug::TIP_STATE_FLAGS);
    // One per line: the captions say what the byte means, which takes more room than a
    // half column of a three-column panel has.
    for (label, tip, on) in flags {
        check(ui, label, on, ui.current_column_width() - 10.0);
        theme::hint(ui, tip);
    }

    ui.unindent_by(gutter);
    ui.next_column();
    ui.indent_by(gutter);
    ui.text_colored(
        Role::Dim.rgba(1.0),
        strings::debug::ADDITIONAL_INFO,
    );
    let info = [
        (strings::debug::INFO_EXTERNAL_TYPE, external_type.to_string()),
        (strings::debug::INFO_LOGICAL_OP, format!("0x{logical:02X}")),
        (
            strings::debug::INFO_NOT_FLAG,
            if not_flag { "true" } else { "false" }.to_string(),
        ),
        (strings::debug::INFO_SCM_FUNCTION, scm_function.to_string()),
    ];
    let label_room = info
        .iter()
        .map(|(label, _)| ui.calc_text_size(*label)[0])
        .fold(0.0_f32, f32::max)
        + 14.0;
    for (label, value) in &info {
        pair(ui, label_room, label, value);
    }

    ui.unindent_by(gutter);
    ui.columns(1, "fields_end", false);
}

/// The thread at `pointer`: the identity a window is keyed by, the name to show, and which
/// half of the queue it is from.
fn link_target(pointer: usize) -> Option<(String, String, Kind)> {
    threads::at(pointer).map(|thread| (thread.key, thread.name, thread.kind))
}

/// A label and its value on one line, the value starting at the same x every time.
fn pair(ui: &Ui, label_room: f32, label: &str, value: &str) {
    let start = ui.cursor_pos()[0];
    ui.text_colored(Role::Dim.rgba(1.0), label);
    theme::same_line_at(ui, start + label_room);
    ui.text_colored(Role::Text.rgba(1.0), value);
}

/// A read-only checkbox: the flag's box, its tick, and its label.
fn check(ui: &Ui, label: &str, on: bool, width: f32) {
    let width = width.max(40.0);
    let line_height = ui.text_line_height();
    let side = theme::flag_box_side(line_height);
    let top = ui.cursor_screen_pos();
    let draw = ui.get_window_draw_list();

    // The same box the sidebar's fields list draws, so a flag reads the same in both panels.
    theme::flag_box(
        &draw,
        [top[0], top[1] + (line_height - side) * 0.5],
        side,
        on,
    );
    draw.add_text([top[0] + side + 7.0, top[1]], theme::dim_color(), label);
    ui.dummy([width, line_height]);
}

