use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};

use crate::icons::{self, Icon};
use crate::strings;
use imgui::{
    Context, DrawListMut, FontConfig, FontGlyphRanges, FontId, FontSource, FontStackToken,
    ImColor32, StyleColor, StyleVar, Ui,
};

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
                rounding: 6.0,
                frame_rounding: 5.0,
                padding: [10.0, 8.0],
                frame_padding: [8.0, 4.0],
                item_spacing: [8.0, 6.0],
                scrollbar: 10.0,
                hairline: true,
                uppercase: true,
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
                rounding: 6.0,
                frame_rounding: 5.0,
                padding: [12.0, 10.0],
                frame_padding: [10.0, 6.0],
                item_spacing: [10.0, 8.0],
                scrollbar: 10.0,
                hairline: true,
                uppercase: true,
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
                rounding: 6.0,
                frame_rounding: 5.0,
                padding: [10.0, 8.0],
                frame_padding: [8.0, 4.0],
                item_spacing: [8.0, 6.0],
                scrollbar: 10.0,
                hairline: true,
                uppercase: true,
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
}

static CURRENT: AtomicU8 = AtomicU8::new(Theme::DEFAULT as u8);

// --------------------------------------------------------------------- spacing

/// The closest a hand-placed row may sit to the edge it is aligned against. Child windows
/// and cards pad themselves wider than this; this is the floor.
pub const PAD: f32 = 4.0;

/// Alpha of the background fills, driven by the opacity slider. It has to reach the palette
/// rather than just the window: child windows paint themselves from `ChildBg` and do not
/// inherit the parent's alpha.
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

/// The card colour: what a floating panel paints itself with.
pub fn panel_color() -> ImColor32 {
    rgb(palette().panel)
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

/// The theme's highlight colour, for the parts of a panel that say "here".
pub fn accent_color() -> ImColor32 {
    rgb(palette().accent)
}

/// Text that sits on an accent fill: the palette's background, dark in every theme.
pub fn on_accent_color() -> ImColor32 {
    rgb(palette().bg)
}

pub fn with_alpha(color: ImColor32, alpha: f32) -> [f32; 4] {
    let c = color.to_rgba_f32s();
    [c[0], c[1], c[2], alpha]
}

/// A palette role, for call sites that pick a colour at run time.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Text,
    Dim,
    Success,
    Warning,
    Error,
}

impl Role {
    pub fn color(self) -> ImColor32 {
        match self {
            Role::Text => text_color(),
            Role::Dim => dim_color(),
            Role::Success => success_color(),
            Role::Warning => warning_color(),
            Role::Error => error_color(),
        }
    }

    /// The same colour in the `[r, g, b, a]` form `text_colored` and the draw list take.
    pub fn rgba(self, alpha: f32) -> [f32; 4] {
        with_alpha(self.color(), alpha)
    }
}

/// A colour as the frame's alpha leaves it.
///
/// ImGui fades disabled widgets through the style alpha, which only its own widgets obey;
/// anything painted straight into a draw list keeps full colour unless it asks here.
fn fading(color: ImColor32, alpha: f32) -> ImColor32 {
    let c = color.to_rgba_f32s();
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    ImColor32::from_rgba(
        channel(c[0]),
        channel(c[1]),
        channel(c[2]),
        channel(c[3] * alpha),
    )
}

// ---------------------------------------------------------------------- style

/// Replace the whole ImGui style with the one belonging to the current palette.
/// Called on every theme change, so nothing from the previous palette can survive.
pub fn apply(ctx: &mut Context) {
    let p = palette();

    let mut style = *ctx.style();
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

/// Latin-1 plus `…` (U+2026), which [`crate::utils::elide_end`] appends to shortened text.
static GLYPH_RANGES: [u32; 5] = [0x0020, 0x00FF, 0x2026, 0x2026, 0];

fn glyph_ranges() -> FontGlyphRanges {
    FontGlyphRanges::from_slice(&GLYPH_RANGES)
}

/// Candidate paths for the proportional UI font; the bundled monospace font is the fallback.
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
                // ImGui keeps a pointer into the data for the process lifetime.
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
                config: Some(FontConfig {
                    glyph_ranges: glyph_ranges(),
                    ..Default::default()
                }),
            }]);
        }
        None => {
            log::warn!("dashboard: no system UI font found, falling back to the bundled one");
            ctx.fonts().add_font(&[FontSource::TtfData {
                data: mono_bytes,
                size_pixels: 24.0,
                config: Some(FontConfig {
                    glyph_ranges: glyph_ranges(),
                    ..Default::default()
                }),
            }]);
        }
    }

    let mono = ctx.fonts().add_font(&[FontSource::TtfData {
        data: mono_bytes,
        size_pixels: 21.0,
        config: Some(FontConfig {
            glyph_ranges: glyph_ranges(),
            ..Default::default()
        }),
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

/// Draw `text` glyph by glyph with `tracking` between letters, and return its width. ImGui
/// has no letter-spacing setting.
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

/// The width [`draw_tracked`] will occupy.
fn tracked_width(ui: &Ui, text: &str, tracking: f32) -> f32 {
    text.chars()
        .map(|ch| ui.calc_text_size(ch.to_string().as_str())[0] + tracking)
        .sum::<f32>()
        - tracking
}

/// A section header: a tracked, uppercased caption and a hairline to the right edge.
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
    let width = tracked_width(ui, &label, tracking);
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

/// The tab strip: an icon and a label per tab, the active one on a filled accent block.
///
/// Returns the index the user clicked, `*selected` otherwise.
pub fn tabs(ui: &Ui, id: &str, items: &[(&str, Icon)], selected: &mut usize) -> usize {
    let p = palette();
    if *selected >= items.len() {
        *selected = 0;
    }

    let icon_size = (ui.text_line_height() * 1.15).round();
    let height = (ui.text_line_height() + 18.0).round().max(34.0);
    let pad_x = 14.0;
    let gap = 8.0;
    let widths: Vec<f32> = items
        .iter()
        .map(|(label, _)| ui.calc_text_size(*label)[0] + icon_size + gap + pad_x * 2.0)
        .collect();

    let origin = ui.cursor_screen_pos();
    let mut clicked = None;

    {
        let _spacing = ui.push_style_var(StyleVar::ItemSpacing([6.0, 0.0]));

        for (i, (label, icon)) in items.iter().enumerate() {
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

            // The active tab is an accent block with its label cut out of it; the rest are
            // captions until the pointer reaches them.
            let color = if active {
                on_accent_color()
            } else if hovered {
                rgb(p.text)
            } else {
                rgb(p.dim)
            };

            let draw = ui.get_window_draw_list();
            if active {
                draw.add_rect(min, max, rgb(p.accent))
                    .filled(true)
                    .rounding(p.frame_rounding)
                    .build();
            } else if hovered {
                draw.add_rect(min, max, rgba(p.frame, 0.75))
                    .filled(true)
                    .rounding(p.frame_rounding)
                    .build();
            }

            // The icon and the label sit together in the middle of the block, not each in
            // its own half: the strip reads as a row of buttons, not as a grid.
            let text = ui.calc_text_size(*label);
            let inner = icon_size + gap + text[0];
            let left = min[0] + (max[0] - min[0] - inner) * 0.5;
            let middle = (min[1] + max[1]) * 0.5;
            icons::draw(
                &draw,
                *icon,
                [left, middle - icon_size * 0.5],
                icon_size,
                color,
            );
            draw.add_text(
                [left + icon_size + gap, middle - text[1] * 0.5],
                color,
                *label,
            );
        }
    }

    // Hairline under the whole strip.
    let right = ui.content_region_max()[0];
    ui.get_window_draw_list()
        .add_line(
            [origin[0], origin[1] + height + 3.0],
            [right, origin[1] + height + 3.0],
            rgb(p.separator),
        )
        .build();

    ui.new_line();
    ui.dummy([0.0, p.item_spacing[1]]);

    // Remember it: the return value alone only survives this frame.
    if let Some(index) = clicked {
        *selected = index;
    }
    *selected
}

/// A filter pill, filled with the accent when it is the selected one.
pub fn chip(ui: &Ui, label: &str, active: bool) -> bool {
    let p = palette();
    // The frame height, so a chip on a row with a text box is exactly as tall as it is.
    let height = ui.frame_height();
    let pad = 15.0;
    let text = ui.calc_text_size(label);
    let width = text[0] + pad * 2.0;
    let origin = ui.cursor_screen_pos();

    let clicked = ui.invisible_button(format!("##chip_{label}"), [width, height]);
    let hovered = ui.is_item_hovered();

    let draw = ui.get_window_draw_list();
    let min = origin;
    let max = [origin[0] + width, origin[1] + height];
    let radius = height * 0.5;
    if active {
        draw.add_rect(min, max, rgb(p.accent))
            .filled(true)
            .rounding(radius)
            .build();
    } else {
        draw.add_rect(min, max, rgba(p.frame, if hovered { 1.0 } else { 0.55 }))
            .filled(true)
            .rounding(radius)
            .build();
    }

    // A pill reads the way a tab does.
    let color = if active {
        on_accent_color()
    } else if hovered {
        rgb(p.text)
    } else {
        rgb(p.dim)
    };
    draw.add_text(
        [
            min[0] + (width - text[0]) * 0.5,
            min[1] + (height - text[1]) * 0.5,
        ],
        color,
        label,
    );

    clicked
}

/// A square button whose face is an icon. Returns whether it was clicked.
pub fn icon_button(ui: &Ui, id: &str, icon: Icon, size: f32, color: ImColor32) -> bool {
    let origin = ui.cursor_screen_pos();
    let box_size = size + 10.0;
    let clicked = ui.invisible_button(id, [box_size, box_size]);
    let hovered = ui.is_item_hovered();

    let tint = fading(
        if hovered { text_color() } else { color },
        ui.clone_style().alpha,
    );
    icons::draw(
        &ui.get_window_draw_list(),
        icon,
        [
            origin[0] + (box_size - size) * 0.5,
            origin[1] + (box_size - size) * 0.5,
        ],
        size,
        tint,
    );
    clicked
}

/// A close button: the `×` glyph, red under the pointer. Every close button in the menu is
/// this one, so they cannot drift apart.
pub fn close_button(ui: &Ui, id: &str, glyph_size: f32) -> bool {
    let p = palette();
    let origin = ui.cursor_screen_pos();
    let glyph = glyph_size.round();
    let side = glyph + 10.0;
    let clicked = ui.invisible_button(id, [side, side]);
    let hovered = ui.is_item_hovered();

    let draw = ui.get_window_draw_list();
    if hovered {
        draw.add_rect(
            origin,
            [origin[0] + side, origin[1] + side],
            rgba(p.error, 0.20),
        )
        .filled(true)
        .rounding(5.0)
        .build();
    }

    icons::draw(
        &draw,
        Icon::Close,
        [
            origin[0] + (side - glyph) * 0.5,
            origin[1] + (side - glyph) * 0.5,
        ],
        glyph,
        // No hover feedback is enough for a close button, so the error colour is its rest
        // state too.
        if hovered { rgb(p.error) } else { rgb(p.accent) },
    );

    clicked
}

/// A section head with a leading icon: the icon and the tracked caption, an optional
/// trailing caret, and a hairline running to the right edge in between.
///
/// `caret` draws a disclosure arrow and makes the row clickable; `reserve` keeps the hit
/// area clear of a button drawn beside the head.
pub fn section_head(
    ui: &Ui,
    title: &str,
    icon: Option<Icon>,
    caret: Option<bool>,
    reserve: f32,
    hint: Option<&str>,
    color: ImColor32,
) -> bool {
    let p = palette();
    let height = ui.text_line_height() + 10.0;
    let origin = ui.cursor_screen_pos();
    let width = ui.content_region_avail()[0];
    let clicked = ui.invisible_button(
        format!("##head_{title}"),
        [(width - reserve).max(0.0), height],
    );

    let icon_size = (ui.text_line_height() * 1.05).round();
    let gap = 8.0;
    let label = if p.uppercase {
        title.to_uppercase()
    } else {
        title.to_string()
    };
    let tracking = if p.uppercase { 1.2 } else { 0.0 };

    let text_start = origin[0] + if icon.is_some() { icon_size + gap } else { 0.0 };
    let text_y = origin[1] + (height - ui.text_line_height()) * 0.5;

    let draw = ui.get_window_draw_list();
    if let Some(icon) = icon {
        icons::draw(
            &draw,
            icon,
            [origin[0], origin[1] + (height - icon_size) * 0.5],
            icon_size,
            color,
        );
    }

    let mut x = text_start;
    for ch in label.chars() {
        let glyph = ch.to_string();
        draw.add_text([x, text_y], color, glyph.as_str());
        x += ui.calc_text_size(glyph.as_str())[0] + tracking;
    }
    let text_width = x - tracking - text_start;

    let mut hairline_right = origin[0] + width;
    if let Some(open) = caret {
        let caret_size = (ui.text_line_height() * 0.9).round();
        let caret_x = origin[0] + width - caret_size;
        icons::draw(
            &draw,
            if open { Icon::CaretDown } else { Icon::CaretRight },
            [caret_x, origin[1] + (height - caret_size) * 0.5],
            caret_size,
            // The one part of the row that does something.
            accent_color(),
        );
        hairline_right = caret_x - 10.0;
    }

    // A key hint sits between the title and the rule: it belongs to the header, not to
    // whatever is under it, and the rule starts after it so the two cannot collide.
    let mut line_start = text_start + text_width + 12.0;
    if let Some(hint) = hint {
        let hint_x = text_start + text_width + 8.0;
        draw.add_text([hint_x, text_y], rgb(p.dim), hint);
        line_start = hint_x + ui.calc_text_size(hint)[0] + 12.0;
    }
    if hairline_right > line_start + 16.0 {
        draw.add_line(
            [line_start, origin[1] + height * 0.5],
            [hairline_right, origin[1] + height * 0.5],
            rgb(p.separator),
        )
        .build();
    }

    clicked
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
    // Off, the track is the frame colour and reads as nothing at all; a dim outline is what
    // says there is a switch there before the pointer arrives. On, the accent fill is the
    // outline's job already.
    if !*value {
        draw.add_rect(min, max, rgba(p.dim, 0.7))
            .thickness(1.0)
            .rounding(height * 0.5)
            .build();
    }
    draw.add_circle(center, radius, knob)
        .filled(true)
        .num_segments(24)
        .build();

    clicked
}

/// Whether the wheel was moved over the list being drawn, in either direction. Used by
/// panes that follow a marker somewhere in the middle.
pub fn wheel_scrolled(ui: &Ui) -> bool {
    ui.io().mouse_wheel != 0.0 && ui.is_window_hovered() && ui.scroll_max_y() > 1.0
}

/// [`wheel_scrolled`] for the wheel **up** only: the log tail pins itself to the newest
/// line, so a wheel down there asks for more of what is already showing.
pub fn scrolled_up(ui: &Ui) -> bool {
    ui.io().mouse_wheel > 0.0 && ui.is_window_hovered() && ui.scroll_max_y() > 1.0
}

/// Show `tooltip` while the previous item is hovered. Has to be called right after that
/// item: ImGui's "last item" decides whether the tooltip shows.
pub fn hint(ui: &Ui, tooltip: &str) {
    if !ui.is_item_hovered() {
        return;
    }

    let wrap = (ui.io().display_size[0] * 0.45).max(260.0);
    ui.tooltip(|| {
        let _wrap = ui.push_text_wrap_pos_with_pos(wrap);
        ui.text(tooltip);
    });
}

/// Fill of the row under the mouse pointer. Faint on purpose: it shows where the eye is
/// rather than selecting anything.
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

/// [`flat_button`] with an icon before the label, painted into one invisible item so the
/// button is exactly as wide as the two of them.
pub fn flat_button_icon(ui: &Ui, icon: Icon, label: &str) -> bool {
    let p = palette();
    let style = ui.clone_style();
    let text = ui.calc_text_size(label);
    let icon_size = (ui.text_line_height() * 0.9).round();
    let gap = 7.0;
    let pad = style.frame_padding;
    let width = pad[0] * 2.0 + icon_size + gap + text[0];
    let height = ui.frame_height();
    let origin = ui.cursor_screen_pos();

    let clicked = ui.invisible_button(format!("##flat_icon_{label}"), [width, height]);
    let hovered = ui.is_item_hovered();
    let active = ui.is_item_active();

    let draw = ui.get_window_draw_list();
    if hovered {
        let alpha = if active { 0.32 } else { 0.18 };
        draw.add_rect(origin, [origin[0] + width, origin[1] + height], rgba(p.accent, alpha))
            .filled(true)
            .rounding(p.frame_rounding)
            .build();
    }

    let tint = fading(rgb(p.accent), style.alpha);
    icons::draw(
        &draw,
        icon,
        [
            origin[0] + pad[0],
            origin[1] + (height - icon_size) * 0.5,
        ],
        icon_size,
        tint,
    );
    draw.add_text(
        [
            origin[0] + pad[0] + icon_size + gap,
            origin[1] + (height - text[1]) * 0.5,
        ],
        tint,
        label,
    );

    clicked
}

/// A child window that paints no background of its own.
///
/// Every child window fills itself from `ChildBg`; a wrapper nested in the sheet would
/// stack the same colour again. Zero padding on purpose: what it wraps is a table or a log,
/// both of which bring their own inset.
pub fn bare_child(ui: &Ui, id: &str, body: impl FnOnce()) {
    let _clear = ui.push_style_color(StyleColor::ChildBg, [0.0, 0.0, 0.0, 0.0]);
    ui.child_window(id).build(body);
}

/// [`bare_child`] with the cards' inner padding: for a pane whose content is a block of text
/// or a column of controls rather than a table.
pub fn padded_child(ui: &Ui, id: &str, body: impl FnOnce()) {
    let _clear = ui.push_style_color(StyleColor::ChildBg, [0.0, 0.0, 0.0, 0.0]);
    let _pad = ui.push_style_var(StyleVar::WindowPadding([8.0, PAD]));
    ui.child_window(id)
        .always_use_window_padding(true)
        .build(body);
}

/// A rounded card: a child window with the panel colour, used to group toolbars and similar
/// clusters. It scrolls vertically when its content does not fit.
pub fn card(ui: &Ui, id: &str, height: f32, body: impl FnOnce()) {
    card_with(ui, id, height, false, true, body);
}

/// A card whose height is the height of its content, so it never scrolls. The scrollbars are
/// off because a card measured to fit exactly is where padding can push the last item a
/// fraction of a pixel past the content region - which ImGui answers with a scrollbar.
pub fn card_fixed(ui: &Ui, id: &str, height: f32, body: impl FnOnce()) {
    card_with(ui, id, height, false, false, body);
}

/// A card that also scrolls sideways, for content that can be wider than the pane.
pub fn card_wide(ui: &Ui, id: &str, height: f32, body: impl FnOnce()) {
    card_with(ui, id, height, true, true, body);
}

fn card_with(ui: &Ui, id: &str, height: f32, scroll_x: bool, scroll_y: bool, body: impl FnOnce()) {
    let p = palette();
    let _bg = ui.push_style_color(StyleColor::ChildBg, rgba(p.panel, opacity()));
    let _pad = ui.push_style_var(StyleVar::WindowPadding([8.0, PAD]));
    ui.child_window(id)
        .size([ui.content_region_avail()[0], height])
        .horizontal_scrollbar(scroll_x)
        .scroll_bar(scroll_x || scroll_y)
        .border(false)
        // A child window ignores `WindowPadding` unless it is told to use it.
        .always_use_window_padding(true)
        .build(body);
}

/// Fill of the row the sidebar is showing. Stronger than the hover band: one says "the
/// mouse is here", the other "this is what the panel is talking about".
pub fn row_selected_color() -> [f32; 4] {
    rgba(palette().accent, 0.16)
}

/// Scroll the current child just enough to show the band `top..top + height`.
///
/// `view_top` is where the visible part begins: the window itself for a plain list, the
/// bottom of the frozen header for a table. A band already inside the view is left alone -
/// centring every step would throw the view around on a one-line move.
pub fn reveal_y(ui: &Ui, top: f32, height: f32, view_top: f32) {
    let view_top = view_top.max(ui.window_pos()[1]);
    let view_bottom = ui.window_pos()[1] + ui.window_size()[1];
    let bottom = top + height;

    let delta = if bottom > view_bottom {
        bottom - view_bottom
    } else if top < view_top {
        top - view_top
    } else {
        return;
    };

    ui.set_scroll_y(ui.scroll_y() + delta);
}

/// [`field`] with an icon button at the right end of the row: returns whether the row took a
/// click and whether the button did.
pub fn action_pair(
    ui: &Ui,
    label: &str,
    value: &str,
    value_color: ImColor32,
    action: Option<(Icon, &str)>,
) -> (bool, bool) {
    row(ui, label, dim_color(), value, value_color, action)
}

/// A named reading: the label dimmed, the value at full contrast. The same rule every row in
/// the menu follows, [`action_pair`] included.
pub fn field(ui: &Ui, label: &str, value: &str) {
    let _ = row(ui, label, dim_color(), value, text_color(), None);
}

/// [`field`], with an icon button at the right end of the row.
pub fn field_action(
    ui: &Ui,
    label: &str,
    value: &str,
    action: Option<(Icon, &str)>,
) -> (bool, bool) {
    row(ui, label, dim_color(), value, text_color(), action)
}

/// The one row behind [`action_pair`], [`field`] and [`field_action`].
fn row(
    ui: &Ui,
    label: &str,
    label_color: ImColor32,
    value: &str,
    value_color: ImColor32,
    action: Option<(Icon, &str)>,
) -> (bool, bool) {
    let origin = ui.cursor_screen_pos();
    let room = ui.content_region_avail()[0];
    let height = ui.text_line_height();
    let clicked = ui.invisible_button(format!("##pair_{label}"), [room, height]);

    let icon_size = height.round();
    let reserved = if action.is_some() { icon_size + 8.0 } else { 0.0 };

    let draw = ui.get_window_draw_list();
    draw.add_text(origin, label_color, label);

    // Pushed after the label is drawn, so the label keeps the proportional face and only
    // the value is monospaced.
    let _mono = mono_font(ui);
    let budget = (room - reserved - ui.calc_text_size(label)[0] - 10.0).max(24.0);
    let shown = crate::utils::elide_end(ui, value, budget);
    let width = ui.calc_text_size(&shown)[0];
    draw.add_text(
        [origin[0] + room - reserved - width, origin[1]],
        value_color,
        shown.as_str(),
    );

    let Some((icon, _tip)) = action else {
        return (clicked, false);
    };

    let min = [origin[0] + room - icon_size, origin[1]];
    let mouse = ui.io().mouse_pos;
    let over = mouse[0] >= min[0]
        && mouse[0] < min[0] + icon_size
        && mouse[1] >= min[1]
        && mouse[1] < min[1] + height;
    let hot = over && ui.is_item_hovered();
    let glyph = (icon_size * 0.62).round();
    icons::draw(
        &draw,
        icon,
        [min[0] + (icon_size - glyph) * 0.5, min[1] + (height - glyph) * 0.5],
        glyph,
        if hot { text_color() } else { accent_color() },
    );

    (clicked, hot && clicked)
}

/// The side a flag box takes on a line of `line_height`. One number for the sidebar's fields
/// list and the debugger's flags, which show the same bits.
pub fn flag_box_side(line_height: f32) -> f32 {
    (line_height * 0.62).round().max(8.0)
}

/// A flag box: filled in the success colour when the bit is set, a dim outline when it is
/// not. Set and clear differ in shape as well as brightness, so a long list stays readable.
pub fn flag_box(draw: &DrawListMut, at: [f32; 2], side: f32, on: bool) {
    let p = palette();
    let corner = [at[0] + side, at[1] + side];
    if on {
        draw.add_rect(at, corner, rgb(p.success))
            .filled(true)
            .rounding(2.0)
            .build();
    } else {
        draw.add_rect(at, corner, rgb(p.dim))
            .thickness(1.0)
            .rounding(2.0)
            .build();
    }
}

/// A row of the fields list: a square lit when the value is set, its name, and the value on
/// the right edge.
pub fn flag_row(ui: &Ui, label: &str, on: bool, text: &str) {
    let p = palette();
    let origin = ui.cursor_screen_pos();
    let room = ui.content_region_avail()[0];
    let height = ui.text_line_height();
    ui.invisible_button(format!("##flag_{label}"), [room, height]);

    let color = if on { rgb(p.success) } else { rgb(p.text) };
    let square = flag_box_side(height);
    let square_y = origin[1] + (height - square) * 0.5;

    let draw = ui.get_window_draw_list();
    flag_box(&draw, [origin[0], square_y], square, on);
    draw.add_text([origin[0] + square + 10.0, origin[1]], dim_color(), label);
    let width = ui.calc_text_size(text)[0];
    draw.add_text([origin[0] + room - width, origin[1]], color, text);
}

/// A row of filter chips for a table toolbar, drawn on the row the search box is on.
/// `tooltip` supplies the hint shown while a chip is hovered; `selected` is the index of the
/// one that is filled.
pub fn chips<T: Copy>(
    ui: &Ui,
    items: &[(T, &'static str)],
    tooltip: impl Fn(T) -> Option<&'static str>,
    selected: &mut usize,
) {
    for (index, (chip, label)) in items.iter().enumerate() {
        ui.same_line();
        if self::chip(ui, label, *selected == index) {
            *selected = index;
        }
        if let Some(tip) = tooltip(*chip) {
            hint(ui, tip);
        }
    }
}

/// The height the tab strip takes, plus the gap it leaves under itself.
pub fn tabs_height(ui: &Ui) -> f32 {
    (ui.text_line_height() + 18.0).round().max(34.0) + palette().item_spacing[1] + 6.0
}

/// A button that opens a menu: its label, and a caret at the right end that says so.
///
/// `id` names the button and has to be unique per instance: a table holds one dropdown per
/// row and they all read the same until a row's own value is picked.
pub fn menu_button(ui: &Ui, id: &str, label: &str) -> bool {
    let p = palette();
    let pad = 12.0;
    let caret = (ui.text_line_height() * 0.8).round();
    let height = ui.frame_height();
    let text = ui.calc_text_size(label);
    let width = text[0] + caret + 16.0 + pad * 2.0;
    let origin = ui.cursor_screen_pos();

    let clicked = ui.invisible_button(format!("##menu_{id}"), [width, height]);
    let hovered = ui.is_item_hovered();

    let draw = ui.get_window_draw_list();
    draw.add_rect(
        origin,
        [origin[0] + width, origin[1] + height],
        rgba(p.frame, if hovered { 1.0 } else { 0.85 }),
    )
    .filled(true)
    .rounding(p.frame_rounding)
    .build();

    draw.add_text(
        [origin[0] + pad, origin[1] + (height - text[1]) * 0.5],
        rgb(p.text),
        label,
    );
    icons::draw(
        &draw,
        Icon::CaretDown,
        [
            origin[0] + width - pad - caret,
            origin[1] + (height - caret) * 0.5,
        ],
        caret,
        rgb(p.accent),
    );

    clicked
}

/// The width [`menu_button`] and [`dropdown`] take for `label`. Exported so a column
/// measured from the text it prints adds the same box the widget draws.
pub fn dropdown_width(ui: &Ui, label: &str) -> f32 {
    let pad = 12.0;
    let caret = (ui.text_line_height() * 0.8).round();
    ui.calc_text_size(label)[0] + caret + 16.0 + pad * 2.0
}

/// [`menu_button`] with the list it opens under it. `id` names both the button and its
/// popup, so it has to be unique per instance; an empty `tooltip` shows none.
pub fn dropdown(ui: &Ui, id: &str, current: &str, tooltip: &str, body: impl FnOnce()) {
    if menu_button(ui, id, current) {
        ui.open_popup(id);
    }
    if !tooltip.is_empty() {
        hint(ui, tooltip);
    }
    if let Some(_popup) = ui.begin_popup(id) {
        body();
    }
}

/// `same_line_with_pos` taking an x in *window* coordinates.
///
/// ImGui's `SameLine` adds the current column's offset to the x it is given, while
/// `cursor_pos` already includes it; without taking the offset back off, a cell placed at an
/// absolute x inside a column lands one column too far right.
pub fn same_line_at(ui: &Ui, window_x: f32) {
    ui.same_line_with_spacing(window_x - ui.current_column_offset(), 0.0);
}
