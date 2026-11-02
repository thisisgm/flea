//! The pending column keeps its frame and facts while the shared Omi marks its wait.
use super::*;
use gpui::{div, prelude::*, px};

impl SelectionPreview {
    pub(crate) fn loading(&self) -> bool {
        self.pending.is_some() && (!self.port.0.borrow().holding || self.drawn.is_none())
    }

    /// The pending row after the hold expires, otherwise the retained frame.
    pub(crate) fn presented(&self) -> Option<&Frame> {
        if self.loading() {
            self.pending.as_ref()
        } else {
            self.drawn.as_ref()
        }
    }

    pub(crate) fn loading_frame(&self, frame: &Frame) -> bool {
        self.loading()
            && self
                .pending
                .as_ref()
                .is_some_and(|pending| pending.generation == frame.generation)
    }

    pub(super) fn loading_view(&self, ink: gpui::Hsla) -> gpui::Div {
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
        let prepared = self.loading_mark.clone();
        let painted = self.loading_mark.clone();
        let mark = gpui::canvas(
            move |bounds, window, cx| {
                prepared.place(bounds, window);
                prepared.prepare(true, None, window, cx);
            },
            move |_, _, window, _| painted.paint(window),
        )
        .size_full();
        div()
            .debug_selector(|| "column-preview-waiting".into())
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(crate::feedback::spinner::GAP))
            .text_color(ink)
            .child(
                div()
                    .debug_selector(|| "column-preview-loading-mark".into())
                    .w(px(crate::feedback::spinner::HERO))
                    .h(px(crate::feedback::spinner::HERO))
                    .flex_none()
                    .child(mark),
            )
            .child(self.catalog.tr("pane.reading"))
            .child(loading_mark)
    }
}
