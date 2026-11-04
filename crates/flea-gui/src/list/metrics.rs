//! Fixed row geometry from the resolved OEM font tokens.
use super::Typeface;
use gpui::{point, px, size, Bounds, Pixels};

/// The pane's geometry at the face's body size, from v0.3.12 Theme.qml's formulas, which list.html and list-textsize.html draw.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    pub row: f32,
    pub nominal_row: f32,
    pub header: f32,
    pub header_top: f32,
    pub pad: f32,
    pub slot: f32,
    pub mark: f32,
    pub gap: f32,
    pub edge: f32,
    pub lane: f32,
    pub clip: f32,
    pub handle: f32,
    pub hairline: f32,
}

// Theme.qml: bodySmall is Omarchy's 0.917 step, a line box is 1.8 of it, and the chrome is 0.72 of a padded row.
const BODY_SMALL_RATIO: f32 = 0.917;
const LINE_BOX_RATIO: f32 = 1.8;
const CHROME_ROW_RATIO: f32 = 0.72;
// Density's Compact keeps half the control padding, Omarchy's 6 px at its stock base of 12.
const COMPACT_PADDING: f32 = 0.5;
pub(crate) const CONTROL_PADDING_Y: f32 = 6.;
pub(crate) const STOCK_BASE: f32 = 12.;
// Theme.markSize from the type scale (Glyph.qml caps it), f64 as QML computes; `as f32` gives the same literal.
pub(crate) const MARK_RATIO: f64 = 1.45;

impl Metrics {
    pub fn at(body: Pixels) -> Self {
        Self::sized(body, px((f32::from(body) * BODY_SMALL_RATIO).round()))
    }

    pub fn for_typeface(face: &Typeface) -> Self {
        Self::sized(face.base, face.body_small)
    }

    fn sized(body: Pixels, small: Pixels) -> Self {
        let body = f32::from(body);
        let board = |value: f32| (value * body / 14.).round();
        let small = f32::from(small);
        let line = (small * LINE_BOX_RATIO).round();
        let padding = (CONTROL_PADDING_Y * body / STOCK_BASE).round();
        let slot = line;
        let nominal_row = line + 2. * padding;
        Self {
            row: line + 2. * (padding * COMPACT_PADDING).round(),
            nominal_row,
            header: (nominal_row * CHROME_ROW_RATIO).round(),
            header_top: board(4.),
            pad: board(14.),
            slot,
            mark: (small * MARK_RATIO as f32).min(slot),
            gap: board(9.),
            edge: board(2.),
            lane: board(14.),
            // Theme.clipMarkSize and the resize zone are fixed pixels, as are hairlines.
            clip: 12.,
            handle: 9.,
            hairline: 1.,
        }
    }

    pub fn label_x(&self) -> f32 {
        self.pad + self.slot + self.gap
    }
}

pub(crate) fn mark_bounds_at(left: Pixels, top: Pixels, metrics: &Metrics) -> Bounds<Pixels> {
    Bounds::new(
        point(
            left + px(metrics.pad + (metrics.slot - metrics.mark) / 2.),
            top + px((metrics.row - metrics.mark) / 2.),
        ),
        size(px(metrics.mark), px(metrics.mark)),
    )
}
