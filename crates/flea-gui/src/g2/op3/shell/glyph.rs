//! The boards' marks, stroked from their own 24 unit paths.
use super::*;
use gpui::{canvas, point, prelude::*, px, Bounds, Hsla, PathBuilder, Pixels, Window};

/// One stroke of a mark in its 24 unit box: a polyline, closed or open; an open one gets square caps.
const SEARCH: &[(&[(f32, f32)], bool)] = &[(&[(16.7, 16.7), (21., 21.)], false)];
const FILE: &[(&[(f32, f32)], bool)] = &[(&[(4., 22.), (4., 2.), (14., 2.), (20., 8.), (20., 22.)], true), (&[(14., 2.), (14., 8.), (20., 8.)], false)];
const CODE: &[(&[(f32, f32)], bool)] = &[(&[(16., 18.), (22., 12.), (16., 6.)], false), (&[(8., 6.), (2., 12.), (8., 18.)], false)];
const APP_WINDOW: &[(&[(f32, f32)], bool)] = &[(&[(3., 4.), (21., 4.), (21., 20.), (3., 20.)], true), (&[(3., 9.), (21., 9.)], false), (&[(6., 6.5), (6.01, 6.5)], false), (&[(9., 6.5), (9.01, 6.5)], false)];
const CHECK_MARK: &[(&[(f32, f32)], bool)] = &[(&[(5., 12.), (10., 17.), (19., 7.)], false)];

/// A board mark at `size` px in `ink`, stroked at the board's width: 2 units, 3.4 for the check.
pub fn glyph(mark: Glyph, size: f32, ink: Hsla) -> impl IntoElement {
    canvas(|_, _, _| (), move |bounds, (), window, _| paint_glyph(mark, bounds, size, ink, window)).size(px(size)).flex_none()
}

fn paint_glyph(mark: Glyph, bounds: Bounds<Pixels>, size: f32, ink: Hsla, window: &mut Window) {
    let unit = size / 24.;
    let (strokes, width) = match mark {
        Glyph::Search => (SEARCH, 2.),
        Glyph::File => (FILE, 2.),
        Glyph::Code => (CODE, 2.),
        Glyph::AppWindow => (APP_WINDOW, 2.),
        Glyph::Check => (CHECK_MARK, 3.4),
    };
    let at = |(x, y): (f32, f32)| point(bounds.origin.x + px(x * unit), bounds.origin.y + px(y * unit));
    let mut path = PathBuilder::stroke(px(width * unit));
    for (points, closed) in strokes {
        let mut points: Vec<(f32, f32)> = points.to_vec();
        if !closed {
            square_caps(&mut points, width / 2.);
        }
        path.move_to(at(points[0]));
        for p in &points[1..] {
            path.line_to(at(*p));
        }
        if *closed {
            path.close();
        }
    }
    if mark == Glyph::Search {
        // The lens: a circle of radius 8 about (11, 11), as the board's two half arcs draw it.
        path.move_to(at((3., 11.)));
        path.arc_to(point(px(8. * unit), px(8. * unit)), px(0.), true, false, at((19., 11.)));
        path.arc_to(point(px(8. * unit), px(8. * unit)), px(0.), true, false, at((3., 11.)));
        path.close();
    }
    if let Ok(path) = path.build() {
        window.paint_path(path, ink);
    }
}

/// Square caps: each open end runs on by half the stroke width along its own segment.
fn square_caps(points: &mut [(f32, f32)], half: f32) {
    let last = points.len() - 1;
    let reach = |from: (f32, f32), to: (f32, f32)| {
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let length = (dx * dx + dy * dy).sqrt().max(f32::EPSILON);
        (to.0 + dx / length * half, to.1 + dy / length * half)
    };
    points[0] = reach(points[1], points[0]);
    points[last] = reach(points[last - 1], points[last]);
}
