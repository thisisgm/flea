//! The sheet's scroll: the resting grid or the results under the wheel, and the highlighted result kept in view.
use super::{SheetView, KEY_ROW};
use crate::g2::st1::wheel::travel;
use gpui::{div, prelude::*, px, Context, Div, ScrollWheelEvent, Window};

impl SheetView {
    /// The clipped body under the card's head, its content drawn at the sheet's scroll.
    pub(super) fn body(&self, content: Div, cx: &mut Context<Self>) -> Div {
        let content = content
            .absolute()
            .left_0()
            .right_0()
            .top(px(-self.sheet.scroll));
        self.probe
            .attach(div(), "body")
            .flex_1()
            .min_h_0()
            .relative()
            .overflow_hidden()
            .on_scroll_wheel(cx.listener(Self::wheel))
            .child(self.probe.attach(content, "content"))
    }

    /// How far the body can scroll, from the latest frame's boxes.
    pub(super) fn most(&self) -> f32 {
        match (self.probe.bounds("body"), self.probe.bounds("content")) {
            (Some(body), Some(content)) => {
                f32::from(content.size.height - body.size.height).max(0.)
            }
            _ => 0.,
        }
    }

    /// Before a frame: a key that moved the highlight brings its row into view, and the scroll stays inside the content.
    pub(super) fn laid_out(&mut self, window: &Window) {
        let viewport = self
            .probe
            .bounds("body")
            .map_or_else(|| self.viewport(window), |body| f32::from(body.size.height));
        if std::mem::take(&mut self.sheet.moved) && !self.sheet.query.is_empty() {
            self.sheet.reveal(viewport, KEY_ROW);
        }
        self.sheet.scroll = self.sheet.scroll.min(self.most());
        self.probe.clear();
    }

    fn wheel(&mut self, event: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.sheet.scroll = (self.sheet.scroll + travel(event.delta)).clamp(0., self.most());
        cx.notify();
    }
}
