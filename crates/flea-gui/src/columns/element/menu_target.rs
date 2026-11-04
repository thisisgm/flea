//! The neighbour menu target uses the existing 2 px accent frame.
use crate::columns::paint::EDGE;
use gpui::{fill, point, px, size, Bounds, Hsla, Pixels, Window};

pub(super) fn paint(frame: Bounds<Pixels>, ink: Hsla, window: &mut Window) {
    let edge = px(EDGE);
    window.paint_quad(fill(
        Bounds::new(frame.origin, size(frame.size.width, edge)),
        ink,
    ));
    window.paint_quad(fill(
        Bounds::new(
            point(frame.left(), frame.bottom() - edge),
            size(frame.size.width, edge),
        ),
        ink,
    ));
    window.paint_quad(fill(
        Bounds::new(frame.origin, size(edge, frame.size.height)),
        ink,
    ));
    window.paint_quad(fill(
        Bounds::new(
            point(frame.right() - edge, frame.top()),
            size(edge, frame.size.height),
        ),
        ink,
    ));
}
