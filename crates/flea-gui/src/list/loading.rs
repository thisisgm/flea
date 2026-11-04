//! Shared geometry for a row whose facts have not landed.
use super::Metrics;
use gpui::{point, px, size, Bounds, Pixels};

const LOADING_WIDTH: f32 = 0.3;

/// Row.qml's loading bar for a row the pane does not hold yet: 30 percent of the row, caption high, right of the mark slot.
pub fn loading_bar(
    left: Pixels,
    top: Pixels,
    metrics: &Metrics,
    row_width: f32,
    caption: f32,
) -> Bounds<Pixels> {
    Bounds::new(
        point(
            left + px(metrics.label_x()),
            top + px((metrics.row - caption) / 2.),
        ),
        size(px(row_width * LOADING_WIDTH), px(caption)),
    )
}
