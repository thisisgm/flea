//! The code viewport reuses the content lane's geometry, pointer handling and idle hold.
use super::CodeView;
use crate::g2::quick_look_overlay::scroller::{Axis, View};
use gpui::{Bounds, Context, Pixels, Point, ScrollWheelEvent};
use std::time::Duration;

impl CodeView {
    pub(crate) fn vertical_lane(
        &self,
    ) -> Option<crate::g2::quick_look_overlay::scroller::capture::Lane> {
        self.scroll.vertical_lane(true)
    }

    pub(super) fn scroll_inset(&self) -> f32 {
        if self.slot == "look" {
            self.metrics.top
        } else {
            0.
        }
    }

    // A wrapped line occupies its fraction of the file without shaping unseen lines to measure a scroll bar.
    pub(super) fn fit_scroll(&mut self, bounds: Bounds<Pixels>) {
        let row = self.metrics.row.max(1.);
        let shown = self.lines.shown();
        let start =
            self.top as f64 + f64::from(self.offset / (self.rows_of(self.top) as f32 * row));
        let mut remaining = f64::from(bounds.size.height);
        let mut end = start;
        let mut line = self.top;
        while remaining > 0. && line < shown {
            let height = f64::from(self.rows_of(line) as f32 * row);
            let offset = if line == self.top {
                f64::from(self.offset)
            } else {
                0.
            };
            let used = remaining.min(height - offset).max(0.);
            end += used / height;
            remaining -= used;
            line += 1;
        }
        let visible = (end - start).max(f64::EPSILON);
        let scale = f64::from(bounds.size.height) / visible;
        self.scroll.y = start * scale;
        self.scroll.fit(View {
            bounds,
            content_w: 0.,
            content_h: if self.refusal().is_some() {
                0.
            } else {
                shown as f64 * scale
            },
            sideways: false,
        });
    }

    pub(super) fn reveal_scroll(&mut self, cx: &mut Context<Self>) {
        let generation = self.scroll.moved(Axis::Vertical);
        self.scroll_timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(
                    flea::ui_logic_preview_core::scroll::HOLD_MS,
                ))
                .await;
            let _ = this.update(cx, |view, cx| {
                if view.scroll.settle(Axis::Vertical, generation) {
                    cx.notify();
                }
            });
        }));
    }

    fn follow_scroll(&mut self, cx: &mut Context<Self>) {
        let Some(view) = self.scroll.view else { return };
        if view.content_h <= 0. {
            return;
        }
        let at = (self.scroll.y / view.content_h * self.lines.shown() as f64).max(0.);
        self.top = (at.floor() as usize).min(self.lines.shown().saturating_sub(1));
        self.offset =
            (at.fract() * f64::from(self.rows_of(self.top) as f32 * self.metrics.row)) as f32;
        self.scroll_by(0., cx);
        self.reveal_scroll(cx);
    }

    pub(super) fn scroll_pointer(&mut self, at: Point<Pixels>, held: bool, cx: &mut Context<Self>) {
        let (changed, dragged) = self.scroll.hover(at, held);
        if dragged.is_some() {
            self.follow_scroll(cx);
        }
        if changed {
            cx.notify();
        }
    }

    pub(super) fn scroll_press(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) -> bool {
        if self.scroll.press(at).is_some() {
            self.follow_scroll(cx);
            return true;
        }
        false
    }

    /// Scrolls by rows of the body line, walking line heights so a wrapped line scrolls through its rows.
    pub fn scroll_by(&mut self, pixels: f32, cx: &mut Context<Self>) {
        self.end_pending = false;
        let before = (self.top, self.offset);
        let row = self.metrics.row.max(1.);
        let mut offset = self.offset + pixels;
        while offset < 0. && self.top > 0 {
            self.top -= 1;
            offset += self.rows_of(self.top) as f32 * row;
        }
        offset = offset.max(0.);
        while self.top + 1 < self.lines.shown() && offset >= self.rows_of(self.top) as f32 * row {
            offset -= self.rows_of(self.top) as f32 * row;
            self.top += 1;
        }
        self.offset = offset;
        self.clamp_end();
        if before != (self.top, self.offset) {
            self.reveal_scroll(cx);
        }
        self.flush(cx);
    }

    // The view never scrolls past the point where the file's last line meets the bottom edge.
    fn clamp_end(&mut self) {
        let row = self.metrics.row.max(1.);
        let shown = self.lines.shown();
        let mut room = self.frame.1;
        let mut line = shown;
        while line > 0 && room > 0. {
            line -= 1;
            room -= self.rows_of(line) as f32 * row;
        }
        let (last_top, last_offset) = if room <= 0. { (line, -room) } else { (0, 0.) };
        if self.top > last_top || (self.top == last_top && self.offset > last_offset) {
            self.top = last_top;
            self.offset = last_offset;
        }
    }

    /// End asks directly for the last viewport, without walking or retaining the intervening lines.
    pub fn scroll_to_end(&mut self, cx: &mut Context<Self>) {
        self.end_pending = true;
        self.settle_end();
        self.reveal_scroll(cx);
        self.flush(cx);
    }

    // The last reply can reveal wrapped line heights that were unknown at the jump.
    pub(super) fn settle_end(&mut self) {
        if self.end_pending {
            self.top = self.lines.shown();
            self.offset = 0.;
            self.clamp_end();
            self.end_pending = !self.lines.first_screen(self.view());
        }
    }

    /// j and k: one row of the body line per step.
    pub fn scroll_rows(&mut self, rows: i64, cx: &mut Context<Self>) {
        self.scroll_by(rows as f32 * self.metrics.row, cx);
    }

    pub(super) fn wheel(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let delta = f32::from(crate::input::scroll::wheel_delta(&event.delta).y);
        self.scroll_by(-delta, cx);
    }
}
