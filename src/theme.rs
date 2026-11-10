use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};

use crate::strings;
use imgui::{Context, FontId, FontSource, FontStackToken, ImColor32, StyleColor, StyleVar, Ui};

// ------------------------------------------------------------------- palettes

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Theme {
    Blue = 0,
    Purple = 1,
    Orange = 2,
}

impl Theme {
    /// Offers for the dropdown, in this order.
    pub const ALL: [&'static str; 3] = strings::settings::THEME_NAMES;

    pub const DEFAULT: Theme = Theme::Orange;

    pub fn from_int(value: i32) -> Self {
        match value {
            1 => Theme::Purple,
            2 => Theme::Orange,
            _ => Theme::Blue,
        }
    }

    pub fn label(self) -> &'static str {
        Self::ALL[self as usize]
    }

    fn palette(self) -> Palette {
        match self {
            Theme::Blue => Palette {
                bg: 0x1B1B1E,
                panel: 0x222327,
                frame: 0x26282C,
                frame_hover: 0x2F3237,
                frame_active: 0x373B41,
                border: 0x37393E,
                separator: 0x2E3035,
                text: 0xE6E7EA,
                dim: 0x8B919C,
                accent: 0x4AA3FF,
                success: 0x4CC38A,
                warning: 0xE0A040,
                error: 0xE06C75,
                rounding: 0.0,
                frame_rounding: 0.0,
                padding: [10.0, 8.0],
                frame_padding: [8.0, 4.0],
                item_spacing: [8.0, 6.0],
                scrollbar: 10.0,
                hairline: true,
                uppercase: true,
                underline_tabs: false,
            },
            Theme::Purple => Palette {
                bg: 0x14161E,
                panel: 0x1D202B,
                frame: 0x232735,
                frame_hover: 0x2D3243,
                frame_active: 0x363C51,
                border: 0x2B2F3D,
                separator: 0x292D3A,
                text: 0xF1F3F8,
                dim: 0x9BA1B5,
                accent: 0x8B7DFF,
                success: 0x5BD6A0,
                warning: 0xE8B04B,
                error: 0xF07080,
                rounding: 0.0,
                frame_rounding: 0.0,
                padding: [12.0, 10.0],
                frame_padding: [10.0, 6.0],
                item_spacing: [10.0, 8.0],
                scrollbar: 10.0,
                hairline: true,
                uppercase: true,
                underline_tabs: false,
            },
            Theme::Orange => Palette {
                bg: 0x0A0A0C,
                panel: 0x141418,
                frame: 0x1C1C20,
                frame_hover: 0x27272B,
                frame_active: 0x313135,
                border: 0x2A2A2E,
                separator: 0x26262A,
                text: 0xEDEDED,
                dim: 0x9A9A9A,
                accent: 0xF0B429,
                success: 0x6BD68A,
                warning: 0xE8A03C,
                error: 0xD9503A,
                rounding: 0.0,
                frame_rounding: 0.0,
                padding: [10.0, 8.0],
                frame_padding: [8.0, 4.0],
                item_spacing: [8.0, 6.0],
                scrollbar: 10.0,
                hairline: true,
                uppercase: true,
                underline_tabs: false,
            },
        }
    }
}

#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: u32,
    pub panel: u32,
    pub frame: u32,
    pub frame_hover: u32,
    pub frame_active: u32,
    pub border: u32,
    pub separator: u32,
    pub text: u32,
    pub dim: u32,
    pub accent: u32,
    pub success: u32,
    pub warning: u32,
    pub error: u32,
    pub rounding: f32,
    pub frame_rounding: f32,
    pub padding: [f32; 2],
    pub frame_padding: [f32; 2],
    pub item_spacing: [f32; 2],
    pub scrollbar: f32,
    pub hairline: bool,
    pub uppercase: bool,
    pub underline_tabs: bool,
}

static CURRENT: AtomicU8 = AtomicU8::new(Theme::DEFAULT as u8);

/// Alpha of the menu's own background fills, i.e. the opacity slider.
///
/// It has to reach the palette, not just the window: every child window paints itself
/// from `ChildBg` and a child does not inherit the parent's background alpha, so scaling
/// only the menu window faded the outline and left the content opaque.
static OPACITY: AtomicU32 = AtomicU32::new((0.9f32).to_bits());

pub fn set_opacity(value: f32) {
    OPACITY.store(value.clamp(0.15, 1.0).to_bits(), Ordering::Relaxed);
}

pub fn opacity() -> f32 {
    f32::from_bits(OPACITY.load(Ordering::Relaxed))
}

pub fn current() -> Theme {
    Theme::from_int(CURRENT.load(Ordering::Relaxed) as i32)
}

pub fn set_current(theme: Theme) {
    CURRENT.store(theme as u8, Ordering::Relaxed);
}

pub fn palette() -> Palette {
    current().palette()
}

// -------------------------------------------------------------------- colours

fn rgb(value: u32) -> ImColor32 {
    let r = ((value >> 16) & 0xFF) as u8;
    let g = ((value >> 8) & 0xFF) as u8;
    let b = (value & 0xFF) as u8;
    ImColor32::from_rgb(r, g, b)
}

/// A palette colour as an `[r, g, b, a]` array, for `text_colored` and friends.
fn rgba(value: u32, alpha: f32) -> [f32; 4] {
    let c = rgb(value).to_rgba_f32s();
    [c[0], c[1], c[2], alpha]
}

pub fn text_color() -> ImColor32 {
    rgb(palette().text)
}

pub fn dim_color() -> ImColor32 {
    rgb(palette().dim)
}

pub fn success_color() -> ImColor32 {
    rgb(palette().success)
}

pub fn warning_color() -> ImColor32 {
    rgb(palette().warning)
}

pub fn error_color() -> ImColor32 {
    rgb(palette().error)
}

// ---------------------------------------------------------------------- style

/// Replace the whole ImGui style with the one belonging to the current palette.
/// Called on every theme change, so nothing from the previous palette can survive.
pub fn apply(ctx: &mut Context) {
    let p = palette();

    let mut style = ctx.style().clone();
    style.window_rounding = p.rounding;
    style.child_rounding = p.rounding;
    style.popup_rounding = p.rounding;
    style.frame_rounding = p.frame_rounding;
    style.tab_rounding = p.frame_rounding;
    style.scrollbar_rounding = p.scrollbar * 0.5;
    style.grab_rounding = p.scrollbar * 0.5;

    style.window_border_size = if p.hairline { 1.0 } else { 0.0 };
    style.popup_border_size = if p.hairline { 1.0 } else { 0.0 };
    style.child_border_size = 0.0;
    style.frame_border_size = 0.0;
    style.tab_border_size = 0.0;

    style.window_padding = p.padding;
    style.frame_padding = p.frame_padding;
    style.item_spacing = p.item_spacing;
    style.item_inner_spacing = [6.0, 4.0];
    style.cell_padding = [p.padding[0] * 0.75, 4.0];
    style.columns_min_spacing = 8.0;
    style.scrollbar_size = p.scrollbar;
    style.grab_min_size = p.scrollbar;
    style.indent_spacing = 14.0;

    style.window_title_align = [0.0, 0.5];
    style.button_text_align = [0.5, 0.5];
    style.anti_aliased_fill = true;
    style.anti_aliased_lines = true;
    style.anti_aliased_lines_use_tex = true;

    let dim_a = 0.6;
    style[StyleColor::Text] = rgba(p.text, 1.0);
    style[StyleColor::TextDisabled] = rgba(p.dim, dim_a);
    style[StyleColor::WindowBg] = rgba(p.bg, opacity());
    style[StyleColor::ChildBg] = rgba(p.panel, opacity());
    style[StyleColor::PopupBg] = rgba(p.bg, 0.97);
    style[StyleColor::Border] = rgba(p.border, 1.0);
    style[StyleColor::BorderShadow] = rgba(0x000000, 0.3);
    style[StyleColor::FrameBg] = rgba(p.frame, 1.0);
    style[StyleColor::FrameBgHovered] = rgba(p.frame_hover, 1.0);
    style[StyleColor::FrameBgActive] = rgba(p.frame_active, 1.0);
    style[StyleColor::TitleBg] = rgba(p.bg, 1.0);
    style[StyleColor::TitleBgActive] = rgba(p.panel, 1.0);
    style[StyleColor::TitleBgCollapsed] = rgba(p.bg, 0.8);
    style[StyleColor::MenuBarBg] = rgba(p.bg, 1.0);
    style[StyleColor::ScrollbarBg] = rgba(p.bg, 0.0);
    style[StyleColor::ScrollbarGrab] = rgba(p.dim, 0.35);
    style[StyleColor::ScrollbarGrabHovered] = rgba(p.dim, 0.6);
    style[StyleColor::ScrollbarGrabActive] = rgba(p.accent, 0.9);
    style[StyleColor::CheckMark] = rgba(p.accent, 1.0);
    // Kept translucent: the value text is drawn over the grab.
    style[StyleColor::SliderGrab] = rgba(p.accent, 0.32);
    style[StyleColor::SliderGrabActive] = rgba(p.accent, 0.55);
    style[StyleColor::Button] = rgba(p.frame, 0.85);
    style[StyleColor::ButtonHovered] = rgba(p.frame_hover, 1.0);
    style[StyleColor::ButtonActive] = rgba(p.frame_active, 1.0);
    style[StyleColor::Header] = rgba(p.accent, 0.22);
    style[StyleColor::HeaderHovered] = rgba(p.accent, 0.35);
    style[StyleColor::HeaderActive] = rgba(p.accent, 0.45);
    style[StyleColor::Separator] = rgba(p.separator, 1.0);
    style[StyleColor::SeparatorHovered] = rgba(p.accent, 0.6);
    style[StyleColor::SeparatorActive] = rgba(p.accent, 1.0);
    style[StyleColor::ResizeGrip] = rgba(p.dim, 0.2);
    style[StyleColor::ResizeGripHovered] = rgba(p.accent, 0.6);
    style[StyleColor::ResizeGripActive] = rgba(p.accent, 1.0);
    style[StyleColor::Tab] = rgba(p.frame, 0.0);
    style[StyleColor::TabHovered] = rgba(p.frame, 0.6);
    style[StyleColor::TabActive] = rgba(p.frame, 0.0);
    style[StyleColor::TabUnfocused] = rgba(p.frame, 0.0);
    style[StyleColor::TabUnfocusedActive] = rgba(p.frame, 0.0);
    style[StyleColor::PlotLines] = rgba(p.accent, 1.0);
    style[StyleColor::PlotLinesHovered] = rgba(p.accent, 1.0);
    style[StyleColor::PlotHistogram] = rgba(p.accent, 1.0);
    style[StyleColor::PlotHistogramHovered] = rgba(p.accent, 1.0);
    style[StyleColor::TextSelectedBg] = rgba(p.accent, 0.35);
    style[StyleColor::DragDropTarget] = rgba(p.accent, 1.0);
    style[StyleColor::NavHighlight] = rgba(p.accent, 1.0);
    style[StyleColor::NavWindowingHighlight] = rgba(p.text, 0.7);
    style[StyleColor::NavWindowingDimBg] = rgba(0x000000, 0.4);
    style[StyleColor::ModalWindowDimBg] = rgba(0x000000, 0.5);
    // The grid weight: a line between two rows, one between two columns, and a stronger
    // one under the frozen header - all in the separator colour, so it reads evenly.
    style[StyleColor::TableHeaderBg] = rgba(p.panel, 0.0);
    style[StyleColor::TableBorderStrong] = rgba(p.separator, 1.0);
    style[StyleColor::TableBorderLight] = rgba(p.separator, 1.0);
    style[StyleColor::TableRowBg] = rgba(0x000000, 0.0);
    style[StyleColor::TableRowBgAlt] = rgba(0xFFFFFF, 0.025);

    *ctx.style_mut() = style;
}

// ---------------------------------------------------------------------- fonts

/// `FontId` points into the ImGui atlas, so it is neither `Send` nor `Sync` as far as the
/// compiler knows. Fonts are registered in `initialize` and read from the render thread,
/// which for hudhook is the same (game render) thread, so the assertions below hold.
struct FontSlot(Option<FontId>);
unsafe impl Send for FontSlot {}
unsafe impl Sync for FontSlot {}

static MONO_FONT: std::sync::Mutex<FontSlot> = std::sync::Mutex::new(FontSlot(None));

/// Paths tried, in order, for the proportional UI font. The bundled monospace font is
/// the fallback, so a machine without any of these still boots the menu.
fn ui_font_candidates() -> Vec<String> {
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".to_string());
    let mut candidates = vec![
        format!("{windir}\\Fonts\\SegUIVar.ttf"), // Segoe UI Variable, Windows 11
        format!("{windir}\\Fonts\\segoeui.ttf"),  // Segoe UI, Windows 10
        format!("{windir}\\Fonts\\tahoma.ttf"),
    ];

    // Inter dropped next to CLEO is honoured too (`<cleo dir>\Inter-Regular.ttf`).
    if let Ok(cleo_dir) = std::env::var("CLEO_DIR") {
        candidates.insert(0, format!("{cleo_dir}\\Inter-Regular.ttf"));
    }

    candidates
}

fn load_ui_font() -> Option<(&'static [u8], String)> {
    for path in ui_font_candidates() {
        if let Ok(bytes) = std::fs::read(&path) {
            if bytes.len() > 1024 {
                // ImGui keeps a pointer into this for the lifetime of the process, so it
                // has to outlive the atlas: leak it on purpose.
                let leaked: &'static [u8] = Box::leak(bytes.into_boxed_slice());
                return Some((leaked, path));
            }
        }
    }
    None
}

/// Register the fonts. The proportional one becomes font 0 (the default), the
/// monospace one is used for log viewers and paths.
pub fn register_fonts(ctx: &mut Context) {
    let mono_bytes: &'static [u8] = include_bytes!("../SourceCodePro-Regular.ttf");

    match load_ui_font() {
        Some((data, path)) => {
            log::info!("dashboard: using UI font {path}");
            ctx.fonts().add_font(&[FontSource::TtfData {
                data,
                size_pixels: 24.0,
                config: None,
            }]);
        }
        None => {
            log::warn!("dashboard: no system UI font found, falling back to the bundled one");
            ctx.fonts().add_font(&[FontSource::TtfData {
                data: mono_bytes,
                size_pixels: 24.0,
                config: None,
            }]);
        }
    }

    let mono = ctx.fonts().add_font(&[FontSource::TtfData {
        data: mono_bytes,
        size_pixels: 21.0,
        config: None,
    }]);
    if let Ok(mut slot) = MONO_FONT.lock() {
        *slot = FontSlot(Some(mono));
    }
}

/// Push the monospace font for the duration of the returned token, if it exists.
pub fn mono_font(ui: &Ui) -> Option<FontStackToken<'_>> {
    let id = MONO_FONT.lock().ok().and_then(|slot| slot.0)?;
    Some(ui.push_font(id))
}

// -------------------------------------------------------------------- widgets

/// Draw text one glyph at a time with extra tracking between the letters, and return the
/// width it occupied. ImGui only builds the ASCII glyph range, so a thin-space trick would
/// render as tofu boxes.
fn draw_tracked(
    ui: &Ui,
    pos: [f32; 2],
    color: ImColor32,
    text: &str,
    tracking: f32,
) -> f32 {
    let mut x = pos[0];
    {
        let draw = ui.get_window_draw_list();
        for ch in text.chars() {
            let glyph = ch.to_string();
            draw.add_text([x, pos[1]], color, glyph.as_str());
            x += ui.calc_text_size(glyph.as_str())[0] + tracking;
        }
    }
    x - pos[0] - tracking
}

/// A section header: a dimmed caption, tracked and uppercased for the angular theme,
/// followed by a hairline running to the right edge.
pub fn section(ui: &Ui, title: &str) {
    let p = palette();
    ui.dummy([0.0, 2.0]);

    let label = if p.uppercase {
        title.to_uppercase()
    } else {
        title.to_string()
    };

    let start = ui.cursor_screen_pos();
    let tracking = if p.uppercase { 1.5 } else { 0.0 };
    let color = rgb(p.dim);

    // Lay the row out first, then paint into it: the width has to be known before the
    // widget exists, and both must use the same metrics.
    let mut width = 0.0;
    let chars: Vec<String> = label.chars().map(|c| c.to_string()).collect();
    for glyph in &chars {
        width += ui.calc_text_size(glyph.as_str())[0] + tracking;
    }
    width -= tracking;
    ui.dummy([width, ui.text_line_height()]);
    draw_tracked(ui, start, color, &label, tracking);

    let y = start[1] + ui.text_line_height() * 0.5;
    let right = start[0] + ui.content_region_avail()[0];
    if right > start[0] + width + 24.0 {
        ui.get_window_draw_list()
            .add_line([start[0] + width + 10.0, y], [right, y], rgb(p.separator))
            .build();
    }

    ui.dummy([0.0, 2.0]);
}

/// The tab strip, drawn by hand: no rounded ImGui tabs, an accent bar under the active
/// one, and a hairline across the full width.
///
/// Returns the index the user clicked, `*selected` otherwise.
pub fn tabs(ui: &Ui, id: &str, labels: &[&str], selected: &mut usize) -> usize {
    let p = palette();
    if *selected >= labels.len() {
        *selected = 0;
    }

    let height = ui.text_line_height() + 12.0;
    let pad_x = 12.0;
    let widths: Vec<f32> = labels
        .iter()
        .map(|label| ui.calc_text_size(*label)[0] + pad_x * 2.0)
        .collect();
    let origin = ui.cursor_screen_pos();
    let total: f32 = widths.iter().sum();

    let mut clicked = None;
    {
        let _spacing = ui.push_style_var(StyleVar::ItemSpacing([0.0, 4.0]));

        for (i, label) in labels.iter().enumerate() {
            if i > 0 {
                ui.same_line();
            }

            let active = i == *selected;
            if ui.invisible_button(format!("##{id}_tab_{i}"), [widths[i], height]) {
                clicked = Some(i);
            }

            let min = ui.item_rect_min();
            let max = ui.item_rect_max();
            let hovered = ui.is_item_hovered();
            let text_size = ui.calc_text_size(*label);
            let text_pos = [
                min[0] + (max[0] - min[0] - text_size[0]) * 0.5,
                min[1] + (max[1] - min[1] - text_size[1]) * 0.5,
            ];

            let draw = ui.get_window_draw_list();

            if active && !p.underline_tabs {
                // Angular themes mark the active tab with a filled block instead.
                draw.add_rect([min[0], min[1] + 4.0], [max[0], max[1] - 2.0], rgba(p.accent, 0.16))
                    .filled(true)
                    .rounding(p.frame_rounding)
                    .build();
            } else if hovered && !active {
                draw.add_rect([min[0] + 2.0, min[1] + 4.0], [max[0] - 2.0, max[1] - 2.0], rgba(p.frame, 0.55))
                    .filled(true)
                    .rounding(p.frame_rounding)
                    .build();
            }

            let color = if active {
                if p.underline_tabs {
                    rgb(p.text)
                } else {
                    rgb(p.accent)
                }
            } else if hovered {
                rgb(p.text)
            } else {
                rgb(p.dim)
            };
            draw.add_text(text_pos, color, *label);

            if active && p.underline_tabs {
                draw.add_rect([min[0] + 6.0, max[1] - 2.0], [max[0] - 6.0, max[1]], rgb(p.accent))
                    .filled(true)
                    .rounding(1.0)
                    .build();
            }
        }
    }

    // Hairline under the whole strip.
    ui.get_window_draw_list()
        .add_line(
            [origin[0], origin[1] + height],
            [origin[0] + total, origin[1] + height],
            rgb(p.separator),
        )
        .build();

    ui.new_line();
    ui.dummy([0.0, p.item_spacing[1]]);

    // Remember it: the return value alone only survives this frame, and the strip would
    // snap back to the old tab on the next one.
    if let Some(index) = clicked {
        *selected = index;
    }
    *selected
}

/// A pill switch, the replacement for `[x]` checkboxes.
pub fn switch(ui: &Ui, id: impl AsRef<str>, value: &mut bool) -> bool {
    let p = palette();

    // The same box as a combo or a slider, so the row keeps one baseline; the pill inside
    // is slimmer and vertically centred.
    let box_height = ui.frame_height();
    let height = (ui.text_line_height() + 1.0).round();
    let width = (height * 1.7).round();

    // Position from the cursor, not the item rect: every switch has to start in the same
    // place.
    let origin = ui.cursor_screen_pos();

    let clicked = ui.invisible_button(id.as_ref(), [width, box_height]);
    if clicked {
        *value = !*value;
    }

    let top = origin[1] + (box_height - height) * 0.5;
    let min = [origin[0], top];
    let max = [origin[0] + width, top + height];
    let hovered = ui.is_item_hovered();

    let track = if *value {
        rgb(p.accent)
    } else if hovered {
        rgb(p.frame_hover)
    } else {
        rgb(p.frame)
    };
    let knob = if *value { rgb(p.bg) } else { rgb(p.dim) };
    let radius = height * 0.5 - 1.8;
    let center = [
        if *value {
            max[0] - height * 0.5
        } else {
            min[0] + height * 0.5
        },
        (min[1] + max[1]) * 0.5,
    ];

    let draw = ui.get_window_draw_list();
    draw.add_rect(min, max, track)
        .filled(true)
        .rounding(height * 0.5)
        .build();
    draw.add_circle(center, radius, knob)
        .filled(true)
        .num_segments(24)
        .build();

    clicked
}

/// Show `tooltip` while the previous item is hovered.
///
/// Has to be called immediately after the widget: ImGui's "last item" decides whether the
/// tooltip shows, and drawing anything in between moves it to that instead.
pub fn hint(ui: &Ui, tooltip: &str) {
    if !ui.is_item_hovered() {
        return;
    }

    // A tooltip is sized to its content and never wraps on its own, so a long hint ran
    // off the right edge. Wrap at a fraction of the display instead; an explicit \n still
    // forces a break.
    let wrap = (ui.io().display_size[0] * 0.45).max(260.0);
    ui.tooltip(|| {
        let _wrap = ui.push_text_wrap_pos_with_pos(wrap);
        ui.text(tooltip);
    });
}

/// Fill of the row under the mouse pointer. Faint on purpose: a band behind the text that
/// shows where the eye is, not a selected row.
pub fn row_hover_color() -> [f32; 4] {
    rgba(palette().accent, 0.08)
}

/// A flat button: text only, an accent tint on hover instead of a filled box.
pub fn flat_button(ui: &Ui, label: impl AsRef<str>) -> bool {
    let p = palette();
    let _bg = ui.push_style_color(StyleColor::Button, rgba(p.accent, 0.0));
    let _bg_hover = ui.push_style_color(StyleColor::ButtonHovered, rgba(p.accent, 0.18));
    let _bg_active = ui.push_style_color(StyleColor::ButtonActive, rgba(p.accent, 0.32));
    let _text = ui.push_style_color(StyleColor::Text, rgba(p.accent, 1.0));
    ui.button(label)
}

/// A child window that paints no background of its own.
///
/// Every child window fills itself from `ChildBg`, so a wrapper nested in the menu sheet
/// stacked the same colour again and the content came out denser than the frame around it
/// at low opacity. Only the sheet and the cards carry a fill.
pub fn bare_child(ui: &Ui, id: &str, body: impl FnOnce()) {
    let _clear = ui.push_style_color(StyleColor::ChildBg, [0.0, 0.0, 0.0, 0.0]);
    ui.child_window(id).build(body);
}

/// A rounded card: a child window with the panel colour, used to group the log toolbars
/// and similar clusters.
pub fn card(ui: &Ui, id: &str, height: f32, body: impl FnOnce()) {
    let p = palette();
    let _bg = ui.push_style_color(StyleColor::ChildBg, rgba(p.panel, opacity()));
    let _pad = ui.push_style_var(StyleVar::WindowPadding([8.0, 4.0]));
    ui.child_window(id)
        .size([ui.content_region_avail()[0], height])
        .border(false)
        .build(body);
}

