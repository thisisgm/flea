//! Absolute overlay ground shared by peek heroes.
use gpui::{div, prelude::*};
use std::rc::Rc;
/// A peek track's tile fills the track from its corner: an in-flow child would stand below the 100 percent tall Element.
pub(super) fn tile() -> gpui::Div {
    gpui::div().absolute().top_0().left_0().size_full()
}

/// The loading mark over its line, centred in the track it fills: a peek's tile, or the active track once its listing fell back.
pub(super) fn loading_tile(
    mark: Rc<crate::feedback::spinner::Crawl>,
    typeface: &crate::list::Typeface,
    ink: gpui::Hsla,
    line: gpui::SharedString,
    window: &gpui::Window,
) -> gpui::Div {
    tile()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .child(crate::feedback::spinner::hero(
            mark,
            div()
                .text_size(typeface.caption)
                .font_family(typeface.font.family.clone())
                .text_color(ink)
                .child(line),
            typeface,
            window,
        ))
}
