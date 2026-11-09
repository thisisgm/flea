//! The stack and zoomed page publish their actual viewport to the shared document scroller.
use super::PdfView;
use crate::g2::quick_look_overlay::scroller::{
    overlay::{Overlay, Owner},
    View,
};
use gpui::{Bounds, Context, Pixels};

impl Owner for PdfView {
    fn overlay(&mut self) -> &mut Overlay {
        &mut self.bars
    }
    fn viewport(&self, bounds: Bounds<Pixels>) -> Option<(View, (f64, f64))> {
        let (content, position) = if self.surface.continuous {
            (
                (self.frame.0, self.surface.layout.height),
                (0., self.surface.view_top),
            )
        } else {
            (self.zoomed().1, self.pan)
        };
        Some((
            View {
                bounds,
                content_w: content.0,
                content_h: content.1,
                sideways: true,
            },
            position,
        ))
    }
    fn position(&mut self, position: (f64, f64), cx: &mut Context<Self>) {
        if self.surface.continuous {
            self.surface.scroll_to(position.1);
        } else {
            self.pan = position;
        }
        self.flush(cx);
        cx.notify();
    }
}
