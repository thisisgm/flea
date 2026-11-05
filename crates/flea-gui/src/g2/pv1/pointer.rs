//! Pointer input the shield hands the card: the viewport's bars and pan, then the ground's policy.
use super::*;

impl QuickLook {
    /// One pointer event the shield already consumed: the viewport's wheel, lane and knob first, then the ground's policy.
    pub fn pointer(&mut self, hand: Hand, cx: &mut Context<Self>) {
        if self.subject.is_none() {
            return;
        }
        if self.custom.is_some() {
            self.custom_reveal(cx);
        }
        let Hand {
            pointer,
            at,
            inside,
            wheel,
            pinch_delta,
            held,
        } = hand;
        match pointer {
            Pointer::Release { .. } => {
                let held = self.pressed.take().is_some();
                if self.scroll.release() || held {
                    cx.notify();
                }
            }
            Pointer::Move => {
                let (changed, dragged) = self.scroll.hover(at, held);
                if let Some(axis) = dragged {
                    self.hold(axis, cx);
                }
                for axis in self.scroll.pan_to(at) {
                    self.hold(axis, cx);
                }
                if changed {
                    cx.notify();
                }
            }
            Pointer::Press {
                button: Button::Left,
                ..
            } if self.revealed && inside => {
                if let Some(axis) = self.scroll.press(at) {
                    self.hold(axis, cx);
                } else if self.zoom > 1. {
                    // PreviewImage.qml's Flickable: a zoomed picture pans under a drag.
                    self.scroll.grab(at);
                }
            }
            Pointer::Wheel if self.revealed => {
                for axis in wheel.map(|d| self.scroll.wheel(at, d)).unwrap_or_default() {
                    self.hold(axis, cx);
                }
            }
            Pointer::Pinch if inside => {
                if let Some(delta) = pinch_delta {
                    let doc = self.subject.as_ref().and_then(|s| s.body.doc()).cloned();
                    if !doc.is_some_and(|doc| doc.pinch(delta, cx)) {
                        self.picture_pinch(delta, cx);
                    }
                }
            }
            _ => {}
        }
        match self.ground.handle(pointer, inside, self.revealed) {
            Effect::Swallow => {}
            Effect::Reveal => self.reveal(cx),
            Effect::RevealStrip => self.reveal_strip(cx),
            Effect::Close => self.close(cx),
        }
    }

    /// The view moved on an axis: its bar shows now and leaves HOLD_MS after the last move.
    fn hold(&mut self, axis: Axis, cx: &mut Context<Self>) {
        let generation = self.scroll.moved(axis);
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(flea::ui_logic_pv1::scroll::HOLD_MS))
                .await;
            let _ = this.update(cx, |ql, cx| {
                if ql.scroll.settle(axis, generation) {
                    cx.notify();
                }
            });
        })
        .detach();
    }
}
