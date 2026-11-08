//! The Columns track's glyph runs and its Lucide marks, painted from prepared runs and static SVG paths.
use gpui::{
    point, px, App, Bounds, GlyphRun, Hsla, Pixels, SharedString, TransformationMatrix, Window,
};

/// Paints a prepared line from the tiles its glyph run kept, its baseline centred on a row `band` high.
pub(super) fn paint_line(
    run: &GlyphRun,
    x: Pixels,
    top: Pixels,
    band: f32,
    color: Hsla,
    window: &mut Window,
) {
    let line = run.line();
    let baseline = top + (px(band) - line.ascent - line.descent) / 2. + line.ascent;
    let scale = window.scale_factor();
    let baseline = px((f32::from(baseline) * scale).floor() / scale);
    #[cfg(test)]
    crate::bfx_cols_tests::record_line(run, x, baseline);
    let _ = window.paint_glyph_run(point(x, baseline), run, color);
}

macro_rules! glyph {
    ($key:literal, $d:literal) => {
        (
            $key,
            concat!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" "#,
                r#"stroke-width="2" stroke-linecap="square" stroke-linejoin="miter"><path d=""#,
                $d,
                r#""/></svg>"#
            )
            .as_bytes(),
        )
    };
}

// Lucide's chevron-right, copy, scissors and move-horizontal in the Omarchy cut ui/js/Icons.js carries.
pub const CHEVRON_SVG: (&str, &[u8]) = glyph!("flea-mark:chevron-right", "M9 18l6-6-6-6");
pub const CHEVRON_LEFT_SVG: (&str, &[u8]) = glyph!("flea-mark:chevron-left", "M15 18l-6-6 6-6");
pub const COPY_SVG: (&str, &[u8]) = glyph!("flea-mark:copy", "M9 8h12v13H9z M4 16V3h13");
pub const SCISSORS_SVG: (&str, &[u8]) = glyph!(
    "flea-mark:scissors",
    "M5 3l14 14 M19 3L5 17 M2 17h5v5H2z M17 17h5v5h-5z"
);

pub const MOVE_SVG: (&str, &[u8]) = glyph!(
    "flea-mark:move-horizontal",
    "M4 12h16 M8 8l-4 4 4 4 M16 8l4 4-4 4"
);

pub(super) fn paint_svg(
    glyph: (&'static str, &'static [u8]),
    bounds: Bounds<Pixels>,
    ink: Hsla,
    window: &mut Window,
    cx: &mut App,
) {
    #[cfg(test)]
    crate::bfx_cols_tests::record_mark(glyph.0, bounds);
    let _ = window.paint_svg(
        bounds,
        SharedString::new_static(glyph.0),
        Some(glyph.1),
        TransformationMatrix::unit(),
        ink,
        cx,
    );
}
