//! A tag's state uses a checkmark or mixed bar, with no empty checkbox.
use flea::ui_logic_pl4::tags::{State, SWATCH_SIZE};
use gpui::{fill, point, px, size, App, Bounds, Hsla, Pixels, TransformationMatrix, Window};

pub(super) fn paint(
    state: State,
    edge: Pixels,
    top: Pixels,
    ink: Hsla,
    metrics: flea::ui_logic_mn1::fit::Metrics,
    window: &mut Window,
    cx: &mut App,
) {
    let side = metrics.of(SWATCH_SIZE);
    let at = Bounds::new(
        point(edge - px(side), top + px((metrics.row as f32 - side) / 2.)),
        size(px(side), px(side)),
    );
    match state {
        State::Absent => {}
        State::Applied => {
            let (key, data) = super::glyphs::mark("checkmark");
            let _ = window.paint_svg(at, key, Some(data), TransformationMatrix::unit(), ink, cx);
        }
        State::Mixed => window.paint_quad(fill(
            Bounds::new(
                point(at.left(), at.center().y - px(1.)),
                size(at.size.width, px(2.)),
            ),
            ink,
        )),
    }
}
