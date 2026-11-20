//! The menu's icon set, drawn with the draw list instead of a font or a texture.
//!
//! Shapes are taken from [Bootstrap Icons](https://icons.getbootstrap.com/) and reduced to
//! what survives at 14-20 pixels: the icons live in a 1x1 box that
//! [`draw`] scales into the rectangle the caller asked for, so the same set reads the same at
//! every UI scale.
//!
//! Everything is either a filled convex polygon or a stroke. Nothing is baked into an atlas:
//! a second font would have to be merged into the ImGui atlas for a dozen glyphs, and a
//! texture would have to be created through the render backend, which the plugin has no
//! handle on. Coordinates are pure geometry, so [`shapes`] is testable without a context.

use imgui::{DrawListMut, ImColor32};

/// The glyphs the menu uses, named after their Bootstrap Icons ids.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    /// `play-fill` - the active Scripts tab.
    Play,
    /// `puzzle-fill` - Libraries.
    Puzzle,
    /// `volume-up-fill` - Audio.
    Speaker,
    /// `journal-text` - Logs.
    Journal,
    /// `gear-fill` - Settings.
    Gear,
    /// `search` - inside the filter box.
    Search,
    /// `box` - the Script Info panel.
    Cube,
    /// `caret-down-fill` - an open section, or a dropdown.
    CaretDown,
    /// `caret-right-fill` - a collapsed section.
    CaretRight,
    /// `box-arrow-up-right` - opening something in another window.
    External,
    /// `info-circle` - the footer note.
    Info,
    /// `x-lg` - closing the menu.
    Close,
    /// `arrow-clockwise` - checking for updates.
    Refresh,
    /// `trash3` - clearing the trace.
    Trash,
    /// `clipboard` - copying the trace.
    Clipboard,
    /// `sliders` - the trace's own options.
    Sliders,
    /// `pause-fill` - parking the script.
    Pause,
    /// `arrow-down` - one step further.
    ArrowDown,
    /// `flag-fill` - the script's state flags.
    Flag,
}

/// One primitive of an icon, in the 0..1 box.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    /// A filled convex polygon.
    Fill(Vec<[f32; 2]>),
    /// A stroked segment, with its width.
    Line([f32; 2], [f32; 2], f32),
    /// A stroked open polyline, with its width.
    Stroke(Vec<[f32; 2]>, f32),
    /// A stroked circle: centre, radius, width.
    Ring([f32; 2], f32, f32),
    /// A filled circle.
    Disc([f32; 2], f32),
    /// A stroked arc: centre, radius, first angle, last angle (degrees, y down), width.
    Arc([f32; 2], f32, f32, f32, f32),
    /// A rounded rectangle: min, max, rounding, width - width 0 fills it.
    Rect([f32; 2], [f32; 2], f32, f32),
}

impl Shape {
    /// Every coordinate the shape touches.
    ///
    /// Only the tests use it: the lib itself paints straight from the variants, and the
    /// test crate pulls this module in by path rather than linking the plugin.
    #[allow(dead_code)]
    pub fn points(&self) -> Vec<[f32; 2]> {
        match self {
            Shape::Fill(points) | Shape::Stroke(points, _) => points.clone(),
            Shape::Line(a, b, _) => vec![*a, *b],
            Shape::Ring(centre, ..) | Shape::Disc(centre, ..) | Shape::Arc(centre, ..) => {
                vec![*centre]
            }
            Shape::Rect(min, max, ..) => vec![*min, *max],
        }
    }
}

/// A circle as a polygon, and an arc as a polyline: the draw list has no curve that takes a
/// width, and a chorded circle at 24 segments is smooth at icon sizes.
const SEGMENTS: usize = 28;

fn circle_points(centre: [f32; 2], radius: f32, first: f32, last: f32) -> Vec<[f32; 2]> {
    let count = ((SEGMENTS as f32 * (last - first).abs() / 360.0).ceil() as usize).max(3);
    (0..=count)
        .map(|step| {
            let angle = (first + (last - first) * step as f32 / count as f32).to_radians();
            [
                centre[0] + radius * angle.cos(),
                centre[1] + radius * angle.sin(),
            ]
        })
        .collect()
}

/// A gear tooth: a trapezoid standing on its inner radius, pointing outwards.
fn tooth(angle_deg: f32, inner: f32, outer: f32, half_width_deg: f32) -> Vec<[f32; 2]> {
    let point = |radius: f32, offset: f32| {
        let angle = (angle_deg + offset).to_radians();
        [0.5 + radius * angle.cos(), 0.5 + radius * angle.sin()]
    };
    vec![
        point(inner, -half_width_deg),
        point(outer, -half_width_deg * 0.6),
        point(outer, half_width_deg * 0.6),
        point(inner, half_width_deg),
    ]
}

/// The primitives of an icon, in the 0..1 box.
pub fn shapes(icon: Icon) -> Vec<Shape> {
    const HAIR: f32 = 0.085;
    match icon {
        Icon::Play => vec![Shape::Fill(vec![
            [0.24, 0.10],
            [0.86, 0.50],
            [0.24, 0.90],
        ])],

        // Two bars, `pause-fill`.
        Icon::Pause => vec![
            Shape::Fill(vec![
                [0.20, 0.14],
                [0.42, 0.14],
                [0.42, 0.86],
                [0.20, 0.86],
            ]),
            Shape::Fill(vec![
                [0.58, 0.14],
                [0.80, 0.14],
                [0.80, 0.86],
                [0.58, 0.86],
            ]),
        ],

        // A shaft with a chevron on its end, `arrow-down`.
        Icon::ArrowDown => vec![
            Shape::Line([0.50, 0.10], [0.50, 0.82], HAIR + 0.03),
            Shape::Stroke(
                vec![[0.22, 0.56], [0.50, 0.86], [0.78, 0.56]],
                HAIR + 0.03,
            ),
        ],

        // `flag-fill`: a pole with the fly hanging off it. The fly is one convex quad
        // rather than the wavy `flag-fill` silhouette, whose notch is a reflex corner that
        // a triangle fan would draw inside out; slanting its top and bottom edges the same
        // way is what still reads as cloth at fourteen pixels.
        Icon::Flag => vec![
            Shape::Fill(vec![
                [0.26, 0.08],
                [0.92, 0.16],
                [0.92, 0.50],
                [0.26, 0.42],
            ]),
            Shape::Line([0.20, 0.04], [0.20, 0.96], 0.11),
        ],

        // A body with a tab on top and one on its right: a puzzle piece, without the notch
        // it would need a subtractive shape for.
        Icon::Puzzle => vec![
            Shape::Rect([0.10, 0.16], [0.86, 0.90], 0.06, 0.0),
            Shape::Disc([0.48, 0.16], 0.16),
            Shape::Disc([0.86, 0.53], 0.16),
        ],

        Icon::Speaker => vec![
            Shape::Rect([0.04, 0.36], [0.30, 0.64], 0.02, 0.0),
            Shape::Fill(vec![[0.30, 0.36], [0.60, 0.06], [0.60, 0.94]]),
            Shape::Fill(vec![[0.30, 0.36], [0.60, 0.94], [0.30, 0.64]]),
            Shape::Arc([0.60, 0.50], 0.22, -46.0, 46.0, HAIR),
            Shape::Arc([0.60, 0.50], 0.36, -46.0, 46.0, HAIR),
        ],

        Icon::Journal => vec![
            Shape::Rect([0.14, 0.06], [0.88, 0.94], 0.08, HAIR),
            Shape::Line([0.28, 0.28], [0.74, 0.28], HAIR),
            Shape::Line([0.28, 0.44], [0.74, 0.44], HAIR),
            Shape::Line([0.28, 0.60], [0.74, 0.60], HAIR),
            Shape::Line([0.28, 0.76], [0.58, 0.76], HAIR),
        ],

        // A thick ring is the body of the gear and the hole at once; the teeth are laid
        // around it.
        Icon::Gear => {
            let mut shapes = vec![Shape::Ring([0.5, 0.5], 0.30, 0.15)];
            for tooth_index in 0..8 {
                let angle = tooth_index as f32 * 45.0;
                shapes.push(Shape::Fill(tooth(angle, 0.30, 0.48, 13.0)));
            }
            shapes
        }

        Icon::Search => vec![
            Shape::Ring([0.42, 0.42], 0.28, 0.10),
            Shape::Line([0.63, 0.63], [0.90, 0.90], 0.13),
        ],

        // An isometric cube: the hexagon is the silhouette, the three inner edges are the
        // corner where its faces meet.
        Icon::Cube => vec![
            Shape::Stroke(
                vec![
                    [0.50, 0.04],
                    [0.94, 0.27],
                    [0.94, 0.73],
                    [0.50, 0.96],
                    [0.06, 0.73],
                    [0.06, 0.27],
                    [0.50, 0.04],
                ],
                0.07,
            ),
            Shape::Line([0.06, 0.27], [0.50, 0.50], 0.07),
            Shape::Line([0.94, 0.27], [0.50, 0.50], 0.07),
            Shape::Line([0.50, 0.50], [0.50, 0.96], 0.07),
        ],

        Icon::CaretDown => vec![Shape::Fill(vec![
            [0.20, 0.36],
            [0.80, 0.36],
            [0.50, 0.70],
        ])],

        Icon::CaretRight => vec![Shape::Fill(vec![
            [0.36, 0.20],
            [0.36, 0.80],
            [0.70, 0.50],
        ])],

        Icon::External => vec![
            Shape::Rect([0.08, 0.30], [0.66, 0.92], 0.04, HAIR),
            Shape::Line([0.44, 0.56], [0.92, 0.08], 0.11),
            Shape::Stroke(vec![[0.60, 0.08], [0.92, 0.08], [0.92, 0.40]], 0.11),
        ],

        Icon::Info => vec![
            Shape::Ring([0.5, 0.5], 0.42, 0.08),
            Shape::Line([0.5, 0.44], [0.5, 0.72], 0.11),
            Shape::Disc([0.5, 0.29], 0.075),
        ],

        Icon::Close => vec![
            Shape::Line([0.20, 0.20], [0.80, 0.80], 0.11),
            Shape::Line([0.80, 0.20], [0.20, 0.80], 0.11),
        ],

        // An open circle with an arrowhead on the end that it would keep turning towards.
        Icon::Trash => vec![
            Shape::Line([0.08, 0.22], [0.92, 0.22], 0.09),
            Shape::Stroke(
                vec![[0.38, 0.22], [0.40, 0.08], [0.60, 0.08], [0.62, 0.22]],
                0.08,
            ),
            Shape::Stroke(
                vec![[0.20, 0.30], [0.27, 0.92], [0.73, 0.92], [0.80, 0.30]],
                0.08,
            ),
            Shape::Line([0.42, 0.42], [0.44, 0.80], 0.06),
            Shape::Line([0.58, 0.42], [0.56, 0.80], 0.06),
        ],

        Icon::Clipboard => vec![
            Shape::Stroke(
                vec![
                    [0.22, 0.16],
                    [0.78, 0.16],
                    [0.78, 0.94],
                    [0.22, 0.94],
                    [0.22, 0.16],
                ],
                0.08,
            ),
            Shape::Rect([0.36, 0.04], [0.64, 0.20], 0.04, 0.0),
            Shape::Line([0.34, 0.46], [0.66, 0.46], 0.07),
            Shape::Line([0.34, 0.64], [0.66, 0.64], 0.07),
            Shape::Line([0.34, 0.80], [0.54, 0.80], 0.07),
        ],

        Icon::Sliders => vec![
            Shape::Line([0.08, 0.22], [0.92, 0.22], 0.07),
            Shape::Disc([0.34, 0.22], 0.13),
            Shape::Line([0.08, 0.50], [0.92, 0.50], 0.07),
            Shape::Disc([0.66, 0.50], 0.13),
            Shape::Line([0.08, 0.78], [0.92, 0.78], 0.07),
            Shape::Disc([0.44, 0.78], 0.13),
        ],

        Icon::Refresh => {
            let (radius, from, to) = (0.36f32, -50.0f32, 250.0f32);
            let end = to.to_radians();
            let tip = [
                0.5 + (radius + 0.10) * end.cos(),
                0.5 + (radius + 0.10) * end.sin(),
            ];
            let a = [
                0.5 + (radius + 0.02) * (end - 0.34).cos(),
                0.5 + (radius + 0.02) * (end - 0.34).sin(),
            ];
            let b = [
                0.5 + (radius + 0.02) * (end + 0.30).cos(),
                0.5 + (radius + 0.02) * (end + 0.30).sin(),
            ];
            vec![
                Shape::Arc([0.5, 0.5], radius, from, to, 0.10),
                Shape::Fill(vec![tip, a, b]),
            ]
        }

    }
}

/// Paint `icon` into a square of `size` at `min`.
pub fn draw(list: &DrawListMut, icon: Icon, min: [f32; 2], size: f32, color: ImColor32) {
    let at = |point: &[f32; 2]| [min[0] + point[0] * size, min[1] + point[1] * size];
    // A stroke narrower than a pixel disappears into the background at 100% scaling, so it
    // is clamped up: the icons are drawn at 14-20 px and 1.5 px is the thinnest that reads.
    let width = |width: f32| (width * size).max(1.1);

    for shape in shapes(icon) {
        match shape {
            Shape::Fill(points) => {
                let points: Vec<[f32; 2]> = points.iter().map(at).collect();
                list.add_polyline(points, color).filled(true).build();
            }
            Shape::Line(a, b, w) => {
                list.add_line(at(&a), at(&b), color)
                    .thickness(width(w))
                    .build();
            }
            Shape::Stroke(points, w) => {
                let points: Vec<[f32; 2]> = points.iter().map(at).collect();
                list.add_polyline(points, color).thickness(width(w)).build();
            }
            Shape::Ring(centre, radius, w) => {
                let points = circle_points(centre, radius, 0.0, 360.0);
                let points: Vec<[f32; 2]> = points.iter().map(at).collect();
                list.add_polyline(points, color).thickness(width(w)).build();
            }
            Shape::Arc(centre, radius, from, to, w) => {
                let points = circle_points(centre, radius, from, to);
                let points: Vec<[f32; 2]> = points.iter().map(at).collect();
                list.add_polyline(points, color).thickness(width(w)).build();
            }
            Shape::Disc(centre, radius) => {
                list.add_circle(at(&centre), radius * size, color)
                    .filled(true)
                    .num_segments(16)
                    .build();
            }
            Shape::Rect(a, b, rounding, w) => {
                let rect = list
                    .add_rect(at(&a), at(&b), color)
                    .filled(w <= 0.0)
                    .rounding(rounding * size);
                if w > 0.0 {
                    rect.thickness(width(w)).build();
                } else {
                    rect.build();
                }
            }
        }
    }
}
