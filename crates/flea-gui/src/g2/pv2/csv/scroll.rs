//! Only Quick Look's table has the two document lanes; the column remains a fitted table.
use super::CsvView;
use crate::g2::pv1::scroller::{
    overlay::{Overlay, Owner},
    View,
};
use gpui::{px, Bounds, Context, Pixels};

impl Owner for CsvView {
    fn overlay(&mut self) -> &mut Overlay {
        &mut self.bars
    }
    fn viewport(&self, mut bounds: Bounds<Pixels>) -> Option<(View, (f64, f64))> {
        let g = self.feed.geometry;
        if !g.look || !self.feed.first_screen() {
            return None;
        }
        bounds.origin.y += px(g.head_height as f32);
        bounds.size.height = (bounds.size.height - px(g.head_height as f32)).max(px(0.));
        let width = self.slots.last().map_or(0., |slot| slot.x + slot.w);
        Some((
            View {
                bounds,
                content_w: width,
                content_h: self.feed.body_rows() as f64 * g.row_height,
                sideways: true,
            },
            self.scroll,
        ))
    }
    fn position(&mut self, position: (f64, f64), cx: &mut Context<Self>) {
        self.scroll = position;
        cx.notify();
    }
}
