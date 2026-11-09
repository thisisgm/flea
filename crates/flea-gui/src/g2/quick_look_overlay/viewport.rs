//! The text and diff viewports: where each sits in the card and how much content it scrolls over.
use super::{look, scroller::View, Body, QuickLook};
use gpui::{div, prelude::*, px, Bounds, Pixels, Window};

/// A clipped viewport inside the card's frame: top and bottom insets, the same inset at both sides.
pub(super) fn pane(top: f32, side: f32, bottom: f32) -> gpui::Div {
    div()
        .absolute()
        .top(px(top))
        .left(px(side))
        .right(px(side))
        .bottom(px(bottom))
        .overflow_hidden()
}

impl QuickLook {
    /// The text or diff viewport in window coordinates, and the content it scrolls over; other kinds have none.
    pub(super) fn viewport(&mut self, card: Bounds<Pixels>, window: &mut Window) -> Option<View> {
        if let Some(view) = self.picture_viewport(card, window.scale_factor().into()) {
            return Some(view);
        }
        let inner = Bounds::new(
            card.origin + gpui::point(px(1.), px(1.)),
            card.size - gpui::size(px(2.), px(2.)),
        );
        let rect = |top: f32, side: f32, bottom: f32| {
            Bounds::new(
                inner.origin + gpui::point(px(side), px(top)),
                inner.size - gpui::size(px(2. * side), px(top + bottom)),
            )
        };
        match &self.subject.as_ref()?.body {
            Body::Text { lines, .. } => Some(View {
                bounds: rect(look::TEXT_TOP, look::PAD, look::TEXT_VIEW_BOTTOM),
                content_w: 0.,
                content_h: lines.len() as f64 * f64::from(look::TEXT_LINE),
                sideways: false,
            }),
            Body::Diff { rows, .. } => {
                // CompareView's widest line is the longest by characters, measured once per subject.
                let widest = *self.scroll.widest.get_or_insert_with(|| {
                    let line = rows
                        .iter()
                        .map(|r| r.text.as_str())
                        .max_by_key(|t| t.chars().count())
                        .unwrap_or("");
                    let run = window.text_style().to_run(line.len());
                    let width = window
                        .text_system()
                        .layout_line(line, px(look::BODY), &[run], None)
                        .width;
                    f64::from(f32::from(width))
                });
                let margin = f64::from(look::DIFF_MARGIN);
                Some(View {
                    bounds: rect(look::DIFF_VIEW_TOP, 0., 0.),
                    content_w: f64::from(look::DIFF_TEXT) + widest + f64::from(look::DIFF_END_PAD),
                    content_h: 2. * margin + rows.len() as f64 * f64::from(look::DIFF_LINE),
                    sideways: true,
                })
            }
            _ => None,
        }
    }
}
