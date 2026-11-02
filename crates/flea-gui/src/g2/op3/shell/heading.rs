//! The card title row every card family shares.
use gpui::Div;

/// The shared title and escape hint, using each card family's resolved text styles.
pub(crate) fn card_heading(
    title: &str,
    esc: &str,
    title_text: Div,
    hint_text: Div,
    gap: gpui::Pixels,
) -> Div {
    use gpui::{div, prelude::*};

    title_text
        .flex()
        .items_center()
        .gap(gap)
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .child(title.to_owned()),
        )
        .child(hint_text.ml_auto().child(esc.to_owned()))
}
