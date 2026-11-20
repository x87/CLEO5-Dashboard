//! Shared Table view drawn using ImGui's table API.

use imgui::{
    StyleVar, TableBgTarget, TableColumnFlags, TableColumnSetup, TableFlags, TableRowFlags, Ui,
};

use crate::icons::{self, Icon};
use crate::theme::{self, Role};

/// Caption, and the width the content needs.
pub struct Column {
    caption: &'static str,
    width: f32,
    stretch: bool,
    trailing: bool,
}

impl Column {
    /// Wide enough for the widest content, so nothing is clipped.
    pub fn fixed(caption: &'static str, width: f32) -> Self {
        Self {
            caption,
            width,
            stretch: false,
            trailing: false,
        }
    }

    /// Takes the room the fixed columns leave over. The caller has to elide the text and
    /// can ask [`text_room`] how much fits.
    pub fn stretch(caption: &'static str) -> Self {
        Self {
            caption,
            width: 1.0,
            stretch: true,
            trailing: false,
        }
    }

    /// Marks the last column, which is not resizable: ImGui would otherwise draw its rule
    /// against the scrollbar.
    pub fn trailing(mut self) -> Self {
        self.mark_trailing();
        self
    }

    /// [`trailing`](Self::trailing) for a column list built at run time.
    pub fn mark_trailing(&mut self) {
        self.trailing = true;
    }
}

fn flags() -> TableFlags {
    TableFlags::RESIZABLE
        | TableFlags::SIZING_FIXED_FIT
        | TableFlags::SCROLL_Y
        | TableFlags::BORDERS_INNER_H
        | TableFlags::BORDERS_INNER_V
        | TableFlags::PAD_OUTER_X
        // The gap between columns is part of the widths callers measure.
        | TableFlags::NO_PAD_INNER_X
}

/// Runs `body` inside a table whose first row stays put while the rest scrolls. `body` draws
/// the header with [`header`] and then one row per entry; `height` is the room it gets.
pub fn table(ui: &Ui, id: &str, columns: &[Column], height: f32, body: impl FnOnce(&Ui)) {
    // Both paddings come from this one value: the table's outer inset horizontally, and
    // vertical zero so a row is as tall as the frame of its widest widget.
    let style = ui.clone_style();
    let _pad = ui.push_style_var(StyleVar::CellPadding([style.window_padding[0], 0.0]));

    let Some(_table) = ui.begin_table_with_sizing(id, columns.len(), flags(), [0.0, height], 0.0)
    else {
        return;
    };

    for column in columns {
        let mut flags = if column.stretch {
            TableColumnFlags::WIDTH_STRETCH
        } else {
            TableColumnFlags::WIDTH_FIXED
        };
        if column.trailing {
            flags |= TableColumnFlags::NO_RESIZE;
        }
        ui.table_setup_column_with(TableColumnSetup {
            flags,
            init_width_or_weight: column.width,
            ..TableColumnSetup::new(column.caption)
        });
    }

    ui.table_setup_scroll_freeze(0, 1);

    body(ui);
}

/// Starts the frozen header row and fills it. The first caption is centred because its cells
/// are; a non-empty second element is the caption's tooltip.
pub fn header(ui: &Ui, captions: &[(&str, &str)]) {
    ui.table_next_row_with_flags(TableRowFlags::HEADERS);
    for (index, (text, tip)) in captions.iter().enumerate() {
        ui.table_next_column();
        if index == 0 {
            caption_centered(ui, text);
        } else {
            caption(ui, text);
        }
        if !tip.is_empty() {
            theme::hint(ui, tip);
        }
    }
}

/// The height a table needs for `rows` body rows plus `extra` pixels of content, capped at
/// `max` (the room the page has).
pub fn body_height(ui: &Ui, rows: usize, extra: f32, max: f32) -> f32 {
    (ui.frame_height() * (rows as f32 + 1.0) + extra + 8.0).min(max)
}

/// Starts a body row and gives it its band. Returns the y the row starts at, for
/// [`reveal_row`].
pub fn body_row(ui: &Ui, height: f32, selected: bool) -> f32 {
    ui.table_next_row();
    let top = ui.cursor_screen_pos()[1];
    if selected {
        selected_row(ui);
    } else {
        hovered_row(ui, height);
    }
    top
}

/// The row above a table: a filter box with its magnifier, `width` wide. `focus` puts the
/// keyboard in it; the caller places the chips and the right-hand control after it.
pub fn search_box(ui: &Ui, id: &str, hint: &str, text: &mut String, focus: bool, width: f32) {
    let style = ui.clone_style();
    let icon = (ui.text_line_height() * 0.95).round();
    let _pad = ui.push_style_var(StyleVar::FramePadding([
        icon + 12.0,
        style.frame_padding[1],
    ]));
    if focus {
        ui.set_keyboard_focus_here();
    }
    ui.set_next_item_width(width);
    ui.input_text(id, text).hint(hint).build();

    let min = ui.item_rect_min();
    let max = ui.item_rect_max();
    icons::draw(
        &ui.get_window_draw_list(),
        Icon::Search,
        [min[0] + 9.0, (min[1] + max[1]) * 0.5 - icon * 0.5],
        icon,
        theme::dim_color(),
    );
}

/// Where a right-aligned control of `width` starts, as an offset for `same_line`, unless the
/// row is already past it.
pub fn right_slot(ui: &Ui, origin_x: f32, room: f32, width: f32) -> Option<f32> {
    let target = origin_x + room - width;
    (ui.cursor_screen_pos()[0] < target).then_some(target - origin_x)
}

/// Fills the row under the mouse pointer, so the eye can follow one line across it. Call it
/// right after the row has been started and before any cell.
pub fn hovered_row(ui: &Ui, height: f32) {
    let top = ui.cursor_screen_pos()[1];
    let mouse = ui.io().mouse_pos;
    // Both axes: the row is as wide as the table, and a panel beside it shares its rows.
    let left = ui.window_pos()[0];
    let right = left + ui.window_size()[0];
    if mouse[1] < top
        || mouse[1] >= top + height
        || mouse[0] < left
        || mouse[0] >= right
        || !ui.is_window_hovered()
    {
        return;
    }
    // Handed to the table rather than painted here: it lands in the background channel,
    // under the text, however late in the row this is called.
    ui.table_set_bg_color(TableBgTarget::ROW_BG0, theme::row_hover_color());
}

/// Fills the row the sidebar is describing, so the two always agree on which script is
/// meant. Drawn through the table for the same reason as the hover band.
pub fn selected_row(ui: &Ui) {
    ui.table_set_bg_color(TableBgTarget::ROW_BG0, theme::row_selected_color());
}

/// Scrolls the table just enough to show the row the keys moved to. `top` and `height` are
/// the row's screen rect, `header_bottom` where the frozen header ends.
pub fn reveal_row(ui: &Ui, top: f32, height: f32, header_bottom: f32) {
    theme::reveal_y(ui, top, height, header_bottom);
}

/// A caption centred over its column, matching [`cell_centered`].
pub fn caption_centered(ui: &Ui, text: &str) {    let width = ui.calc_text_size(text)[0];
    let room = ui.content_region_avail()[0];
    let pos = ui.cursor_pos();
    ui.set_cursor_pos([pos[0] + ((room - width) * 0.5).max(0.0), pos[1]]);
    caption(ui, text);
}

/// A caption in the frozen header row.
pub fn caption(ui: &Ui, text: &str) {
    // Shares its baseline with the combo or switch beside it in the same row.
    ui.align_text_to_frame_padding();
    ui.text_colored(Role::Dim.rgba(1.0), text);
}

/// How much text fits in the cell being drawn: `content_region_avail` is already the width
/// the cell can draw on, item spacing and cell padding taken out.
pub fn text_room(ui: &Ui) -> f32 {
    ui.content_region_avail()[0].max(40.0)
}

/// What a column sized from the text it prints has to add back: the cell padding ImGui keeps
/// out of `content_region_avail`, on both sides.
pub fn cell_pad(ui: &Ui) -> f32 {
    ui.clone_style().cell_padding[0] * 2.0
}

/// One text cell: a frame-height invisible box is the item and the text is painted into it,
/// vertically centred. The box keeps the row one frame tall (plain text is positioned from
/// the text base and drifts against frame-height widgets) and doubles as the hover target.
pub fn cell(ui: &Ui, id: &str, text: &str, color: Role, tooltip: &str) {
    cell_parts(ui, id, &[(text, color)], tooltip);
}

/// A text cell made of several colours, e.g. a number and its unit.
pub fn cell_parts(ui: &Ui, id: &str, parts: &[(&str, Role)], tooltip: &str) {
    parts_at(ui, id, parts, tooltip, false, false);
}

/// [`cell_parts`] filling its whole column, and reporting the click.
pub fn cell_parts_fill(ui: &Ui, id: &str, parts: &[(&str, Role)], tooltip: &str) -> bool {
    parts_at(ui, id, parts, tooltip, false, true)
}

/// A text cell centred in its column, for the row-number column.
pub fn cell_centered(ui: &Ui, id: &str, text: &str, color: Role) {
    parts_at(ui, id, &[(text, color)], "", true, false);
}

/// [`cell_centered`] filling its whole column, and reporting the click: the row number and
/// the path are the two places that select a row, and both want the whole cell as the target
/// rather than the glyphs of the text inside it.
pub fn cell_centered_fill(ui: &Ui, id: &str, text: &str, color: Role) -> bool {
    parts_at(ui, id, &[(text, color)], "", true, true)
}

fn parts_at(
    ui: &Ui,
    id: &str,
    parts: &[(&str, Role)],
    tooltip: &str,
    centered: bool,
    fill: bool,
) -> bool {
    let widths: Vec<f32> = parts
        .iter()
        .map(|(text, _)| ui.calc_text_size(*text)[0])
        .collect();
    let text_width: f32 = widths.iter().sum();
    let room = ui.content_region_avail()[0];
    // A filled cell is as wide as its column, so the whole column is the target and the text
    // is centred inside that box at draw time; only the unfilled box travels with the text.
    let width = if fill { room.max(text_width) } else { text_width };
    if centered && !fill {
        let pos = ui.cursor_pos();
        ui.set_cursor_pos([pos[0] + ((room - text_width) * 0.5).max(0.0), pos[1]]);
    }

    let top = ui.cursor_screen_pos();
    let height = ui.frame_height();
    let clicked = ui.invisible_button(id, [width, height]);
    if !tooltip.is_empty() {
        theme::hint(ui, tooltip);
    }

    let y = top[1] + ((height - ui.text_line_height()) * 0.5).round();
    let mut x = if centered {
        top[0] + ((width - text_width) * 0.5).max(0.0)
    } else {
        top[0]
    };
    let draw = ui.get_window_draw_list();
    for ((text, role), width) in parts.iter().zip(&widths) {
        draw.add_text([x, y], role.color(), *text);
        x += width;
    }

    clicked
}
