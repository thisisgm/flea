//! The boards' marks, stroked from their own 24 unit paths.
use super::*;
use gpui::{canvas, point, prelude::*, px, Bounds, Hsla, PathBuilder, Pixels, Window};

/// One stroke of a mark in its 24 unit box: a polyline, closed or open; an open one gets square caps.
const SEARCH: &[(&[(f32, f32)], bool)] = &[(&[(16.7, 16.7), (21., 21.)], false)];
const FILE: &[(&[(f32, f32)], bool)] = &[
    (
        &[(4., 22.), (4., 2.), (14., 2.), (20., 8.), (20., 22.)],
        true,
    ),
    (&[(14., 2.), (14., 8.), (20., 8.)], false),
];
const CODE: &[(&[(f32, f32)], bool)] = &[
    (&[(16., 18.), (22., 12.), (16., 6.)], false),
    (&[(8., 6.), (2., 12.), (8., 18.)], false),
];
const TERMINAL: &[(&[(f32, f32)], bool)] = &[
    (&[(4., 17.), (10., 11.), (4., 5.)], false),
    (&[(12., 19.), (20., 19.)], false),
];
const APP_WINDOW: &[(&[(f32, f32)], bool)] = &[
    (&[(3., 4.), (21., 4.), (21., 20.), (3., 20.)], true),
    (&[(3., 9.), (21., 9.)], false),
    (&[(6., 6.5), (6.01, 6.5)], false),
    (&[(9., 6.5), (9.01, 6.5)], false),
];
const CHECK_MARK: &[(&[(f32, f32)], bool)] = &[(&[(5., 12.), (10., 17.), (19., 7.)], false)];
const EYE: &[(&[(f32, f32)], bool)] = &[
    (&[(12., 5.), (22., 12.), (12., 19.), (2., 12.)], true),
    (&[(10., 10.), (14., 10.), (14., 14.), (10., 14.)], true),
];
const ALERT: &[(&[(f32, f32)], bool)] = &[
    (&[(12., 2.), (23., 21.), (1., 21.)], true),
    (&[(12., 10.), (12., 14.)], false),
    (&[(12., 18.), (12.01, 18.)], false),
];
const DRIVE: &[(&[(f32, f32)], bool)] = &[
    (
        &[
            (6., 4.),
            (18., 4.),
            (22., 12.),
            (22., 20.),
            (2., 20.),
            (2., 12.),
        ],
        true,
    ),
    (&[(2., 12.), (22., 12.)], false),
    (&[(6., 16.), (6.01, 16.)], false),
    (&[(10., 16.), (10.01, 16.)], false),
];
const CROSS: &[(&[(f32, f32)], bool)] = &[
    (&[(6., 6.), (18., 18.)], false),
    (&[(18., 6.), (6., 18.)], false),
];
const FOLDER: &[(&[(f32, f32)], bool)] = &[(
    &[
        (2., 20.),
        (2., 3.),
        (8., 3.),
        (10., 6.),
        (22., 6.),
        (22., 20.),
    ],
    true,
)];
const FOLDER_OPEN: &[(&[(f32, f32)], bool)] = &[
    (
        &[
            (2., 20.),
            (2., 3.),
            (8., 3.),
            (10., 6.),
            (22., 6.),
            (22., 9.),
        ],
        false,
    ),
    (&[(22., 11.), (19.5, 20.), (2., 20.), (4.5, 11.)], true),
];

/// A board mark at `size` px in `ink`, stroked at the board's width: 2 units, 3.4 for the check.
pub fn glyph(mark: Glyph, size: f32, ink: Hsla) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, (), window, _| paint_glyph(mark, bounds, size, ink, window),
    )
    .size(px(size))
    .flex_none()
}

fn paint_glyph(mark: Glyph, bounds: Bounds<Pixels>, size: f32, ink: Hsla, window: &mut Window) {
    let unit = size / 24.;
    let (strokes, width) = match mark {
        Glyph::Search => (SEARCH, 2.),
        Glyph::File => (FILE, 2.),
        Glyph::Code => (CODE, 2.),
        Glyph::Terminal => (TERMINAL, 2.),
        Glyph::AppWindow => (APP_WINDOW, 2.),
        Glyph::Check => (CHECK_MARK, 3.4),
        Glyph::Eye => (EYE, 2.),
        Glyph::Alert => (ALERT, 2.),
        Glyph::Drive => (DRIVE, 2.),
        Glyph::Cross => (CROSS, 2.),
        Glyph::Folder => (FOLDER, 2.),
        Glyph::FolderOpen => (FOLDER_OPEN, 2.),
    };
    let at = |(x, y): (f32, f32)| {
        point(
            bounds.origin.x + px(x * unit),
            bounds.origin.y + px(y * unit),
        )
    };
    let mut path = PathBuilder::stroke(px(width * unit));
    for (points, closed) in strokes {
        if *closed {
            path.move_to(at(points[0]));
            for p in &points[1..] {
                path.line_to(at(*p));
            }
            path.close();
        } else {
            // Square caps without a paint-time copy: each open end runs on along its own segment.
            path.move_to(at(extend(points[1], points[0], width / 2.)));
            for p in &points[1..] {
                path.line_to(at(*p));
            }
            path.line_to(at(extend(
                points[points.len() - 2],
                points[points.len() - 1],
                width / 2.,
            )));
        }
    }
    if mark == Glyph::Search {
        // The lens: a circle of radius 8 about (11, 11), as the board's two half arcs draw it.
        path.move_to(at((3., 11.)));
        path.arc_to(
            point(px(8. * unit), px(8. * unit)),
            px(0.),
            true,
            false,
            at((19., 11.)),
        );
        path.arc_to(
            point(px(8. * unit), px(8. * unit)),
            px(0.),
            true,
            false,
            at((3., 11.)),
        );
        path.close();
    }
    if let Ok(path) = path.build() {
        window.paint_path(path, ink);
    }
}

/// One square-cap reach: the end runs on by half the stroke width along its own segment.
fn extend(from: (f32, f32), to: (f32, f32), half: f32) -> (f32, f32) {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let length = (dx * dx + dy * dy).sqrt().max(f32::EPSILON);
    (to.0 + dx / length * half, to.1 + dy / length * half)
}
