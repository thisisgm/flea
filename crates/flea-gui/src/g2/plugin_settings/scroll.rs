//! The Plugins pane's scroll: the wheel and the cursor's keys over rows measured in the latest frame.
use super::page::Card;
use super::view::PluginsPane;
use crate::g2::settings_pages::wheel::travel;
use gpui::{Context, ScrollWheelEvent, Window};

impl PluginsPane {
    /// How far the rows can scroll, from the latest frame's boxes.
    pub(super) fn most(&self) -> f32 {
        match (self.probe.bounds("pane"), self.probe.bounds("rows")) {
            (Some(pane), Some(rows)) => f32::from(rows.size.height - pane.size.height).max(0.),
            _ => 0.,
        }
    }

    /// Before a frame: rows that shrank since the last frame pull the scroll back before its boxes are cleared.
    pub(super) fn laid_out(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.scroll = self.scroll.min(self.most());
        self.probe.clear();
        // Rows that shrink in this frame are measured only after prepaint: one more frame clamps against those boxes.
        if self.scroll > 0. {
            cx.on_next_frame(window, |pane, _, cx| {
                if pane.scroll > pane.most() {
                    cx.notify();
                }
            });
        }
    }

    /// After a key: a page opened or closed starts at its top, and a cursor move keeps its row in view.
    pub(super) fn keyed(&mut self, title: &str) {
        if self.page.title() != title {
            self.scroll = 0.;
        } else {
            self.reveal();
        }
    }

    /// The scroll after a key moves the cursor: up to its row, or down until the row clears the pane's foot.
    fn reveal(&mut self) {
        let row = self.page.cursor().and_then(|at| self.page.rows().get(at));
        let drawn = row.and_then(|row| self.probe.bounds(&format!("row:{}", row.id())));
        if let (Some(row), Some(rows), Some(pane)) =
            (drawn, self.probe.bounds("rows"), self.probe.bounds("pane"))
        {
            let top = f32::from(row.top() - rows.top());
            let bottom = top + f32::from(row.size.height);
            let viewport = f32::from(pane.size.height);
            if top < self.scroll {
                self.scroll = top;
            } else if bottom > self.scroll + viewport {
                self.scroll = bottom - viewport;
            }
        }
        self.scroll = self.scroll.clamp(0., self.most());
    }

    /// A wheel or touchpad turn over the rows; a card over them keeps it.
    pub(super) fn wheel(
        &mut self,
        event: &ScrollWheelEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if *self.page.card() != Card::None {
            return;
        }
        self.scroll = (self.scroll + travel(event.delta)).clamp(0., self.most());
        cx.notify();
    }
}
