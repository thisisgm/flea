//! The oracle's two-line refusal slot scrolls with the body and never moves the buttons.
use super::{
    drawing::{Drawn, Rect, HEIGHT},
    view::NetworkView,
};
use gpui::{px, Window};

impl Drawn {
    pub(super) fn status(&mut self, view: &NetworkView, window: &Window) {
        let Some(key) = view.model.status else {
            return;
        };
        let text = view.catalog.tr_args(key, &[&view.model.detail]);
        let width = self.card.map_or(499., |r| r.w) - 55.;
        let mut remaining = text.as_str();
        for line in 0..2 {
            let start = self.text.len();
            self.label(
                remaining,
                Rect {
                    x: 38.,
                    y: 544. + line as f32 * 18.,
                    w: width.max(0.),
                    h: 18.,
                },
                view.palette.error_on_surface,
                12.,
                false,
                window,
            );
            let shaped = self.text.pop().unwrap();
            let mut end = shaped
                .run
                .line()
                .closest_index_for_x(px(width.max(0.)))
                .min(remaining.len());
            while !remaining.is_char_boundary(end) {
                end = end.saturating_sub(1);
            }
            if end < remaining.len() && line == 0 {
                if let Some(space) = remaining[..end].rfind(char::is_whitespace) {
                    end = space;
                }
            }
            if end == remaining.len() {
                self.text.push(shaped);
                break;
            }
            let label = if line == 1 {
                format!("{}…", remaining[..end].trim_end())
            } else {
                remaining[..end].into()
            };
            self.label(&label, shaped.rect, shaped.ink, 12., false, window);
            debug_assert_eq!(self.text.len(), start + 1);
            remaining = remaining[end..].trim_start();
        }
    }
}
impl NetworkView {
    pub(super) fn content_height(&self) -> f32 {
        HEIGHT + if self.model.status.is_some() { 18. } else { 0. }
    }
    pub(super) fn show_status(&mut self) {
        if self.model.status.is_some() && self.dimensions.1 > 0. {
            let height = self.body_height(self.dimensions.1);
            self.scroll = self.scroll.max((580. - (height - 47.)).max(0.));
        }
    }
}
