//! A folder target takes the same accent rung and leading edge as the List target.
use super::Ink;
use gpui::{fill, px, size, Bounds, Pixels, Window};

pub(super) fn paint(bounds: Bounds<Pixels>, ink: &Ink, window: &mut Window) {
    window.paint_quad(fill(bounds, ink.accent.opacity(crate::list::CURSOR_WASH)));
    window.paint_quad(fill(
        Bounds::new(bounds.origin, size(px(2.), bounds.size.height)),
        ink.accent_frame,
    ));
}
