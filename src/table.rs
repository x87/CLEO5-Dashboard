//! Shared Table view drawn using ImGui's table API.

use imgui::{
    StyleVar, TableBgTarget, TableColumnFlags, TableColumnSetup, TableFlags, TableRowFlags, Ui,
};

use crate::{color_of, theme, Colors};

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

    /// Marks the last column, which is not resizable: ImGui would otherwise draw its
    /// rule against the scrollbar.
    pub fn trailing(mut self) -> Self {
        self.trailing = true;
        self
    }
}

fn flags() -> TableFlags {
    TableFlags::RESIZABLE // drag the rule between two captions
        | TableFlags::SIZING_FIXED_FIT // keep the given widths instead of stretching them all
        | TableFlags::SCROLL_Y
        | TableFlags::BORDERS_INNER_H // hairline between rows, stronger one under the header
        | TableFlags::BORDERS_INNER_V // rule between columns
        | TableFlags::PAD_OUTER_X // inset around the table, so no cell touches the panel edge
        | TableFlags::NO_PAD_INNER_X // the gap between columns is part of their width
}

/// Runs `body` inside a table whose first row stays put while the rest scrolls.
///
/// `body` draws the header row with [`TableRowFlags::HEADERS`] and then one row per entry.
/// `height` is the room the table gets.
pub fn table(ui: &Ui, id: &str, columns: &[Column], height: f32, body: impl FnOnce(&Ui)) {
    // Both paddings come from this one value: horizontally it is the inset around the
    // table (`NO_PAD_INNER_X` keeps it out of the gaps between columns, which are part of
    // the widths callers measure), vertically zero, so a row is as tall as the frame of
    // the widest widget in it.
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

/// Starts the frozen header row.
pub fn header_row(ui: &Ui) {
    ui.table_next_row_with_flags(TableRowFlags::HEADERS);
}

/// Fills the row under the mouse pointer, so the eye can follow one line across it.
/// Call it right after the row has been started and before any cell.
///
/// `height` cannot be asked of the table before the row has been drawn.
pub fn hovered_row(ui: &Ui, height: f32) {
    let top = ui.cursor_screen_pos()[1];
    let mouse = ui.io().mouse_pos;
    if mouse[1] < top || mouse[1] >= top + height || !ui.is_window_hovered() {
        return;
    }
    // Handed to the table rather than painted here: it lands in the background channel,
    // under the text, however late in the row this is called.
    ui.table_set_bg_color(TableBgTarget::ROW_BG0, theme::row_hover_color());
}

/// A caption centred over its column, matching [`cell_centered`].
pub fn caption_centered(ui: &Ui, text: &str) {
    let width = ui.calc_text_size(text)[0];
    let room = ui.content_region_avail()[0];
    let pos = ui.cursor_pos();
    ui.set_cursor_pos([pos[0] + ((room - width) * 0.5).max(0.0), pos[1]]);
    caption(ui, text);
}

/// A caption in the frozen header row.
pub fn caption(ui: &Ui, text: &str) {
    // Shares its baseline with the combo or switch that sits beside it in the same row.
    ui.align_text_to_frame_padding();
    ui.text_colored(color_of(Colors::Secondary).to_rgba_f32s(), text);
}

/// How much text fits in the cell being drawn, with the column's right-hand gap taken
/// off. Only meaningful inside a table, before the cell's item is added.
pub fn text_room(ui: &Ui) -> f32 {
    (ui.content_region_avail()[0] - ui.clone_style().item_spacing[0] * 2.0).max(40.0)
}

/// One text cell: an invisible frame-height box is the item and the text is painted into
/// it, vertically centred. The box is what keeps a row one frame tall - plain text is
/// positioned from the DC's text base and drifts against frame-height widgets - and it
/// doubles as the hover target for the tooltip.
pub fn cell(ui: &Ui, id: &str, text: &str, color: Colors, tooltip: &str) {
    cell_parts(ui, id, &[(text, color)], tooltip);
}

/// A text cell made of several colours, e.g. a number and its unit.
pub fn cell_parts(ui: &Ui, id: &str, parts: &[(&str, Colors)], tooltip: &str) {
    parts_at(ui, id, parts, tooltip, false);
}

/// A text cell centred in its column, for the row-number column.
pub fn cell_centered(ui: &Ui, id: &str, text: &str, color: Colors) {
    parts_at(ui, id, &[(text, color)], "", true);
}

fn parts_at(ui: &Ui, id: &str, parts: &[(&str, Colors)], tooltip: &str, centered: bool) {
    let widths: Vec<f32> = parts
        .iter()
        .map(|(text, _)| ui.calc_text_size(*text)[0])
        .collect();
    let width: f32 = widths.iter().sum();
    if centered {
        // Centred by moving the cursor, not the text: the box is the hover target, so
        // both have to move together.
        let room = ui.content_region_avail()[0];
        let pos = ui.cursor_pos();
        ui.set_cursor_pos([pos[0] + ((room - width) * 0.5).max(0.0), pos[1]]);
    }

    let top = ui.cursor_screen_pos();
    let height = ui.frame_height();
    ui.invisible_button(id, [width, height]);
    if !tooltip.is_empty() {
        theme::hint(ui, tooltip);
    }

    let y = top[1] + ((height - ui.text_line_height()) * 0.5).round();
    let mut x = top[0];
    let draw = ui.get_window_draw_list();
    for ((text, color), width) in parts.iter().zip(&widths) {
        draw.add_text([x, y], color_of(*color), *text);
        x += width;
    }
}
