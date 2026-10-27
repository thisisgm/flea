//! The column preview's loading line, which clears the frame and play rects and stamps when loading began.
use super::*;
use gpui::{div, prelude::*};

impl SelectionPreview {
    pub(super) fn loading_view(&self, ink: gpui::Hsla) -> gpui::Div {
        *self.frame_rect.borrow_mut() = None;
        *self.play_rect.borrow_mut() = None;
        let port = self.port.clone();
        let epoch = self.epoch;
        let loading_mark = gpui::canvas(
            |_, _, _| (),
            move |_, _, _, cx| {
                let now = cx
                    .background_executor()
                    .now()
                    .saturating_duration_since(epoch)
                    .as_millis() as u64;
                let mut swap = port.0.borrow_mut();
                swap.loading_at.get_or_insert(now);
            },
        )
        .absolute()
        .size_full();
        div()
            .relative()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_color(ink)
            .child(self.catalog.tr("preview.loading"))
            .child(loading_mark)
    }
}
