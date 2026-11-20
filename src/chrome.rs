//! The two painted rows of the sheet: the title bar with the version, update check, frame
//! rate and close button, and the footer band with the page summary and the close hint.

use imgui::Ui;

use crate::icons::{self, Icon};
use crate::overlay;
use crate::settings;
use crate::sdk::CLEO_GetVersionStr;
use crate::strings;
use crate::theme;
use crate::updates;
use crate::utils::{elide_path, ptr_to_rust_str};

fn inside(min: [f32; 2], max: [f32; 2], point: [f32; 2]) -> bool {
    point[0] >= min[0] && point[0] < max[0] && point[1] >= min[1] && point[1] < max[1]
}

pub fn title_height(ui: &Ui) -> f32 {
    (ui.text_line_height() + 26.0).round()
}

/// The header row. Painted rather than laid out: every element sits on one line of its own
/// height, and the right-hand group has to be measured before the centred title can be told
/// where it starts.
pub fn title_bar(ui: &Ui, width: f32, click: bool) {
    let origin = ui.cursor_screen_pos();
    let height = title_height(ui);
    let line = ui.text_line_height();
    let middle = origin[1] + height * 0.5;
    let hovering = ui.is_window_hovered();
    let mouse = ui.io().mouse_pos;

    // Reserving the row first gives every painted element an owner: the dummy is the item.
    ui.dummy([width, height]);

    let close = (line + 14.0).round();
    let close_min = [origin[0] + width - close, middle - close * 0.5];

    let group_start = right_group(ui, close_min[0], middle, line, hovering, mouse, click);
    draw_title(ui, origin, width, height, middle, group_start);
    draw_close(ui, close_min, close, hovering, mouse, click);
}

/// The CLEO version, the update button and the frame rate, right-aligned before the close
/// button. Returns the x the group starts at, which is where the centred title must stop.
#[allow(clippy::too_many_arguments)]
fn right_group(
    ui: &Ui,
    right_edge: f32,
    middle: f32,
    line: f32,
    hovering: bool,
    mouse: [f32; 2],
    click: bool,
) -> f32 {
    let gap = 16.0;
    let version = format!("CLEO {}", ptr_to_rust_str(unsafe { CLEO_GetVersionStr() }));
    let version_size = ui.calc_text_size(&version);

    let icon = (line * 0.95).round();
    let label = strings::footer::CHECK_UPDATES;
    let label_size = ui.calc_text_size(label);
    let button_width = icon + 9.0 + label_size[0] + 18.0;

    let status_label = updates::status().label();
    let status_size = ui.calc_text_size(status_label);

    // The number is laid out three digits wide whatever it reads: the group is right-aligned,
    // so a varying width would step everything beside it sideways.
    let show_fps = settings::get().show_fps;
    let fps_label = strings::footer::FPS_LABEL;
    let fps_value = format!("{:.0}", ui.io().framerate);
    let fps_label_size = ui.calc_text_size(fps_label);
    let fps_digits = ui.calc_text_size("000");
    let fps_value_size = ui.calc_text_size(&fps_value);

    let status_room = if status_label.is_empty() {
        0.0
    } else {
        12.0 + status_size[0]
    };
    let fps_room = if show_fps {
        20.0 + fps_label_size[0] + 5.0 + fps_digits[0]
    } else {
        0.0
    };
    let group = version_size[0] + gap * 2.0 + button_width + status_room + fps_room;
    let group_start = right_edge - 20.0 - group;

    let version_x = group_start;
    let rule_x = version_x + version_size[0] + gap;
    let button_min = [rule_x + gap, middle - (line + 10.0) * 0.5];
    let button_max = [button_min[0] + button_width, middle + (line + 10.0) * 0.5];
    let status_x = button_max[0] + 12.0;
    let fps_x = if status_label.is_empty() {
        button_max[0]
    } else {
        status_x + status_size[0]
    } + 20.0;

    let line_y = |size: [f32; 2]| middle - size[1] * 0.5;
    let draw = ui.get_window_draw_list();

    draw.add_text(
        [version_x, line_y(version_size)],
        theme::text_color(),
        version.as_str(),
    );
    // A hairline where the design has a `|`, lined up with the cap height.
    draw.add_line(
        [rule_x, middle - line * 0.35],
        [rule_x, middle + line * 0.35],
        theme::dim_color(),
    )
    .build();

    let hovered = hovering && inside(button_min, button_max, mouse);
    if hovered {
        draw.add_rect(
            button_min,
            button_max,
            theme::with_alpha(theme::accent_color(), 0.16),
        )
        .filled(true)
        .rounding(5.0)
        .build();
    }
    let tint = if hovered {
        theme::text_color()
    } else {
        theme::accent_color()
    };
    icons::draw(
        &draw,
        Icon::Refresh,
        [button_min[0] + 9.0, middle - icon * 0.5],
        icon,
        tint,
    );
    draw.add_text(
        [button_min[0] + 9.0 + icon + 9.0, line_y(label_size)],
        tint,
        label,
    );
    if click && hovered {
        updates::check();
    }

    if !status_label.is_empty() {
        draw.add_text(
            [status_x, line_y(status_size)],
            theme::success_color(),
            status_label,
        );
    }

    if show_fps {
        draw.add_text([fps_x, line_y(fps_label_size)], theme::dim_color(), fps_label);
        draw.add_text(
            [fps_x + fps_label_size[0] + 5.0, line_y(fps_value_size)],
            theme::text_color(),
            fps_value.as_str(),
        );
    }

    group_start
}

/// The logo and the name, centred in the bar but giving way to the right-hand group.
fn draw_title(ui: &Ui, origin: [f32; 2], width: f32, height: f32, middle: f32, group_start: f32) {
    let logo = (height * 0.64).round();
    let name = strings::title_bar::NAME;
    let name_size = ui.calc_text_size(name);
    let block = logo + 14.0 + name_size[0];
    let block_x = (origin[0] + (width - block) * 0.5)
        .min(group_start - 14.0 - block)
        .max(origin[0]);

    let logo_min = [block_x, middle - logo * 0.5];
    let logo_max = [logo_min[0] + logo, logo_min[1] + logo];
    let draw = ui.get_window_draw_list();
    draw.add_rect(logo_min, logo_max, theme::accent_color())
        .filled(true)
        .rounding(logo * 0.26)
        .build();

    let initial = ui.calc_text_size(strings::title_bar::LOGO);
    draw.add_text(
        [
            logo_min[0] + (logo - initial[0]) * 0.5,
            logo_min[1] + (logo - initial[1]) * 0.5,
        ],
        theme::on_accent_color(),
        strings::title_bar::LOGO,
    );
    draw.add_text(
        [logo_max[0] + 14.0, middle - name_size[1] * 0.5],
        theme::text_color(),
        name,
    );
}

fn draw_close(
    ui: &Ui,
    min: [f32; 2],
    side: f32,
    hovering: bool,
    mouse: [f32; 2],
    click: bool,
) {
    let max = [min[0] + side, min[1] + side];
    let hovered = hovering && inside(min, max, mouse);
    let draw = ui.get_window_draw_list();

    if hovered {
        draw.add_rect(min, max, theme::with_alpha(theme::error_color(), 0.20))
            .filled(true)
            .rounding(5.0)
            .build();
    }

    let glyph = (side * 0.46).round();
    icons::draw(
        &draw,
        Icon::Close,
        [
            min[0] + (side - glyph) * 0.5,
            min[1] + (side - glyph) * 0.5,
        ],
        glyph,
        if hovered {
            theme::error_color()
        } else {
            theme::accent_color()
        },
    );

    if click && hovered {
        overlay::set_open(false);
    }
}

/// The hint on the right of the footer, measured before anything is drawn: it owns the right
/// edge, and the summary on the left elides into whatever it leaves over.
struct Hint<'a> {
    key: &'a str,
    prefix_width: f32,
    key_width: f32,
    middle_width: f32,
    escape_width: f32,
    between_width: f32,
    link_width: f32,
    width: f32,
    fits: bool,
}

impl<'a> Hint<'a> {
    fn measure(ui: &Ui, key: &'a str, room: f32) -> Self {
        let prefix_width = ui.calc_text_size(strings::footer::CLOSE_HINT_BEFORE)[0];
        let key_width = ui.calc_text_size(key)[0];
        let middle_width = ui.calc_text_size(strings::footer::CLOSE_HINT_MIDDLE)[0];
        let escape_width = ui.calc_text_size(strings::footer::CLOSE_HINT_ESCAPE)[0];
        let between_width = ui.calc_text_size(strings::footer::CLOSE_HINT_AFTER)[0];
        let link_width = ui.calc_text_size(strings::footer::SETTINGS_LINK)[0];
        let width = prefix_width
            + key_width
            + middle_width
            + escape_width
            + between_width
            + link_width;

        Self {
            key,
            prefix_width,
            key_width,
            middle_width,
            escape_width,
            between_width,
            link_width,
            width,
            fits: width + 260.0 < room,
        }
    }

    /// Draws the hint and returns whether its `Settings` link was clicked.
    fn draw(&self, ui: &Ui, origin: [f32; 2], after_row: [f32; 2], room: f32) -> bool {
        let height = ui.text_line_height();
        let left = origin[0] + room - self.width;
        let key_left = left + self.prefix_width;
        let middle_left = key_left + self.key_width;
        let escape_left = middle_left + self.middle_width;
        let after_left = escape_left + self.escape_width;
        let link_left = after_left + self.between_width;

        let draw = ui.get_window_draw_list();
        let text = |x: f32, color, value: &str| {
            draw.add_text([x, origin[1]], color, value);
        };
        text(left, theme::dim_color(), strings::footer::CLOSE_HINT_BEFORE);
        text(key_left, theme::accent_color(), self.key);
        text(
            middle_left,
            theme::dim_color(),
            strings::footer::CLOSE_HINT_MIDDLE,
        );
        text(
            escape_left,
            theme::accent_color(),
            strings::footer::CLOSE_HINT_ESCAPE,
        );
        text(
            after_left,
            theme::dim_color(),
            strings::footer::CLOSE_HINT_AFTER,
        );

        // The link is the word `Settings` itself: an invisible button laid over it, drawn in
        // the colour of its own hover state.
        ui.set_cursor_screen_pos([link_left, origin[1]]);
        let clicked = ui.invisible_button("##footer_settings", [self.link_width, height]);
        let hovered = ui.is_item_hovered();
        ui.set_cursor_pos(after_row);
        if hovered {
            theme::hint(ui, strings::footer::TIP_SETTINGS_LINK);
            let underline = origin[1] + height - 1.0;
            draw.add_line(
                [link_left, underline],
                [link_left + self.link_width, underline],
                theme::text_color(),
            )
            .build();
        }

        draw.add_text(
            [link_left, origin[1]],
            if hovered {
                theme::text_color()
            } else {
                theme::accent_color()
            },
            strings::footer::SETTINGS_LINK,
        );

        clicked
    }
}

/// The bottom line: what the page is showing on the left, how to close the menu on the
/// right. `summary` is the page's own line, `note` the Scripts table's remark about dropped
/// columns, `quick` and `toggle` the keys the page answers to. Returns whether the hint's
/// `Settings` link was clicked.
pub fn footer(
    ui: &Ui,
    summary: Option<&str>,
    note: Option<&str>,
    quick: Option<&str>,
    toggle: Option<&str>,
    key: &str,
) -> bool {
    let room = ui.content_region_avail()[0];
    let height = ui.text_line_height();
    let band_top = ui.window_pos()[1];
    let band_height = ui.window_size()[1];
    let origin = [
        ui.cursor_screen_pos()[0],
        band_top + (band_height - height) * 0.5,
    ];

    ui.dummy([room, height]);
    // Where the row leaves the cursor; the link's item must not move it.
    let after_row = ui.cursor_pos();

    let hint = Hint::measure(ui, key, room);
    if let Some(summary) = summary {
        draw_summary(ui, origin, height, room, &hint, summary, [note, quick, toggle]);
    }

    if !hint.fits {
        return false;
    }
    hint.draw(ui, origin, after_row, room)
}

fn draw_summary(
    ui: &Ui,
    origin: [f32; 2],
    height: f32,
    room: f32,
    hint: &Hint,
    summary: &str,
    extras: [Option<&str>; 3],
) {
    let icon = (height * 0.95).round();
    icons::draw(
        &ui.get_window_draw_list(),
        Icon::Info,
        [origin[0], origin[1] + (height - icon) * 0.5],
        icon,
        theme::dim_color(),
    );

    let left = origin[0] + icon + 10.0;
    let right = if hint.fits {
        origin[0] + room - hint.width - 40.0
    } else {
        origin[0] + room
    };
    let shown = elide_path(ui, summary, (right - left).max(80.0));
    ui.get_window_draw_list()
        .add_text([left, origin[1]], theme::dim_color(), shown.as_str());

    let mut x = left + 10.0 + ui.calc_text_size(&shown)[0];
    for text in extras.into_iter().flatten() {
        let width = ui.calc_text_size(text)[0];
        if x + width > right {
            break;
        }
        ui.get_window_draw_list()
            .add_text([x, origin[1]], theme::dim_color(), text);
        x += width;
    }
}
