//! The sheet's scroll: the resting grid or the results under the wheel, and the highlighted result kept in view.
use super::{SheetView, KEY_ROW};
use crate::g2::settings_pages::wheel::travel;
use gpui::{div, prelude::*, px, Context, Div, ScrollWheelEvent, Window};

/// What the last frame laid out, read before the probe clears for this one.
pub(super) struct Measured {
    pub most: f32,
    pub body: f32,
    pub feet: [Option<Foot>; 2],
}

/// A resting column's end and its last row's middle, measured from the content's top.
#[derive(Clone, Copy, PartialEq)]
pub(super) struct Foot {
    pub end: f32,
    pub half: f32,
}

pub(super) const LAST_ROWS: [&str; 2] = ["last-0", "last-1"];
pub(super) const COLUMNS: [&str; 2] = ["column-0", "column-1"];

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

    /// Each resting column's foot as the latest frame laid it out.
    fn feet(&self) -> [Option<Foot>; 2] {
        let content = self.probe.bounds("content");
        [0, 1].map(|side| {
            let (content, column, last) = (
                content?,
                self.probe.bounds(COLUMNS[side])?,
                self.probe.bounds(LAST_ROWS[side])?,
            );
            Some(Foot {
                end: f32::from(column.bottom() - content.top()),
                half: f32::from(last.center().y - content.top()),
            })
        })
    }

    /// Before a frame it reveals a moved key's row, clamps the scroll and returns what the last frame measured.
    pub(super) fn laid_out(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Measured {
        let viewport = self
            .probe
            .bounds("body")
            .map_or_else(|| self.viewport(window), |body| f32::from(body.size.height));
        if std::mem::take(&mut self.sheet.moved) && !self.sheet.query.is_empty() {
            self.sheet.reveal(viewport, KEY_ROW);
        }
        let (most, feet) = (self.most(), self.feet());
        self.sheet.scroll = self.sheet.scroll.min(most);
        self.probe.clear();
        // This frame's boxes exist only after prepaint: draw once more when they moved the end or a foot, so the fades and clamp follow.
        cx.on_next_frame(window, move |view, _, cx| {
            if (view.most() - most).abs() > 0.5 || view.feet() != feet {
                cx.notify();
            }
        });
        Measured {
            most,
            body: viewport,
            feet,
        }
    }

    fn wheel(&mut self, event: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.sheet.scroll = (self.sheet.scroll + travel(event.delta)).clamp(0., self.most());
        cx.notify();
    }
}
