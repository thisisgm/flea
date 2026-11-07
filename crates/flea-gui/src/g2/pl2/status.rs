//! The two-line refusal slot under Mounts as; a shown status grows the card, so the body never scrolls the chips under the title.
use super::{
    drawing::{Drawn, Rect, HEIGHT},
    view::NetworkView,
};
use gpui::{px, Window};

/// Two 18px rows at y 544 and 562, then 22px of air above the buttons: the card grows 39px, from 610 to 649.
const STATUS_GROWTH: f32 = 39.;

impl Drawn {
    pub(super) fn status(&mut self, view: &NetworkView, window: &Window) {
        let Some(key) = view.model.status else {
            return;
        };
        let text = view.catalog.tr_args(key, &[&view.model.detail]);
        // The sign-in invitation is not a failure: muted, with no alert mark, from the fields' own left edge.
        let invited = key == super::mount::GUEST_EMPTY;
        let (x, ink) = if invited {
            (17., view.palette.muted)
        } else {
            (38., view.palette.error_on_surface)
        };
        let width = self.card.map_or(499., |r| r.w) - 17. - x;
        let mut remaining = text.as_str();
        for line in 0..2 {
            let start = self.text.len();
            self.label(
                remaining,
                Rect {
                    x,
                    y: 544. + line as f32 * 18.,
                    w: width.max(0.),
                    h: 18.,
                },
                ink,
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
    pub(super) fn status_growth(&self) -> f32 {
        if self.model.status.is_some() {
            STATUS_GROWTH
        } else {
            0.
        }
    }
    pub(super) fn content_height(&self) -> f32 {
        HEIGHT + self.status_growth()
    }
    /// A status change resizes the card, so the scroll keeps only what the new height still allows.
    pub(super) fn show_status(&mut self) {
        if self.dimensions.1 > 0. {
            let height = self.body_height(self.dimensions.1);
            self.scroll = self
                .scroll
                .clamp(0., (self.content_height() - height).max(0.));
        }
    }
}
