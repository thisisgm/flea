//! The chooser body: the path strip over the places rail and the listing, with the listing's wheel.
use super::picker::Picker;
use crate::g2::pk2::view::ListingView;
use gpui::{div, prelude::*, px, Context, Entity, IntoElement, Render, WeakEntity, Window};

// The path strip's band above the listing, 27 px under the 34 px title on the picker-path board.
const PATH_HEIGHT: f32 = 27.;

// The body slot: the path strip over the places rail and the listing, as picker-open draws them.
pub(super) struct Body {
    pub(super) path: Entity<super::chrome_view::ChromeView>,
    pub(super) rail: Entity<super::rail_view::RailView>,
    pub(super) listing: Entity<ListingView>,
    pub(super) picker: WeakEntity<Picker>,
}

impl Render for Body {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let picker = self.picker.clone();
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex_none()
                    .h(px(PATH_HEIGHT))
                    .child(self.path.clone()),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_row()
                    .child(self.rail.clone())
                    .child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .on_scroll_wheel(move |event, _, cx| {
                                let _ =
                                    picker.update(cx, |picker, cx| picker.listing_wheel(event, cx));
                            })
                            .child(self.listing.clone()),
                    ),
            )
    }
}
