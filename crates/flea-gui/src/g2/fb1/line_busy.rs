//! The existing Omi player in the status lane's compact Starting slot.
use crate::feedback::spinner::{Crawl, GAP, OMI_GRID_PX};
use gpui::{canvas, div, prelude::*, px, IntoElement};
use std::rc::Rc;

pub(super) fn element(mark: Rc<Crawl>) -> impl IntoElement {
    let paint = mark.clone();
    div()
        .id("fb1-busy")
        .debug_selector(|| "fb1-busy".into())
        .size(px(OMI_GRID_PX))
        .mr(px(GAP))
        .flex_none()
        .child(
            canvas(
                move |bounds, window, cx| {
                    mark.place(bounds, window);
                    mark.prepare(true, None, window, cx);
                },
                move |_, _, window, _| paint.paint(window),
            )
            .size_full(),
        )
}
