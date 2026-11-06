//! Trash uses the shared viewport scroller's geometry, with its own position and reveal lifetime.
use super::pane::TrashPane;
use crate::g2::pv1::scroller::{Axis, Scroll, View};
use flea::ui_logic_pv1::scroll;
use gpui::{
    canvas, div, point, prelude::*, px, size, AnyElement, Bounds, Context, DispatchPhase,
    MouseButton, MouseMoveEvent, MouseUpEvent, Pixels, Point, Task, Window,
};
use std::time::{Duration, Instant};

const FADE_MS: u64 = 300;

#[derive(Default)]
pub(super) struct Bar {
    pub(super) model: Scroll,
    previous: Option<(usize, f32)>,
    moved_at: Option<Instant>,
    timer: Option<Task<()>>,
}

impl TrashPane {
    fn fit_scrollbar(&mut self) {
        let paint = self.paint.borrow();
        let bounds = paint.bounds.unwrap_or_else(|| {
            Bounds::new(
                point(px(0.), px(0.)),
                size(px(self.width), px(self.view().height)),
            )
        });
        let model = &mut self.scrollbar.model;
        model.y = f64::from(paint.scroll_y);
        model.fit(View {
            bounds: Bounds::new(
                bounds.origin,
                size(px(self.width), px(self.host.view.height)),
            ),
            content_w: f64::from(self.width),
            content_h: paint.total as f64 * f64::from(paint.board.row_h),
            sideways: false,
        });
    }

    fn reveal_scrollbar(&mut self, cx: &mut Context<Self>) {
        self.scrollbar.moved_at = Some(cx.background_executor().now());
        self.scrollbar.timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(scroll::HOLD_MS))
                .await;
            if this.update(cx, |_, cx| cx.notify()).is_err() {
                return;
            }
            cx.background_executor()
                .timer(Duration::from_millis(FADE_MS))
                .await;
            let _ = this.update(cx, |_, cx| cx.notify());
        }));
    }

    pub(super) fn bar_press(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) -> bool {
        if self.interaction_held() {
            return false;
        }
        self.fit_scrollbar();
        if self.scrollbar.model.press(at).is_none() {
            return false;
        }
        let y = self.scrollbar.model.y as f32;
        self.note_scroll(cx);
        self.host.view.set_viewport(y, self.host.view.height);
        self.reveal_scrollbar(cx);
        self.pump(cx);
        true
    }

    pub(super) fn bar_move(
        &mut self,
        at: Point<Pixels>,
        held: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        self.fit_scrollbar();
        let dragging = self.scrollbar.model.drag.is_some();
        let (changed, moved) = self.scrollbar.model.hover(at, held);
        if self.interaction_held() {
            self.scrollbar.model.release();
        } else if moved.is_some() {
            self.note_scroll(cx);
            self.host
                .view
                .set_viewport(self.scrollbar.model.y as f32, self.host.view.height);
            self.reveal_scrollbar(cx);
            self.pump(cx);
        }
        if changed {
            cx.notify();
        }
        dragging || self.scrollbar.model.lane.is_some()
    }

    pub(super) fn bar_release(&mut self, cx: &mut Context<Self>) {
        if self.scrollbar.model.release() {
            self.reveal_scrollbar(cx);
            cx.notify();
        }
    }

    fn scrollbar_grab(&self, cx: &Context<Self>) -> AnyElement {
        let pane = cx.entity().downgrade();
        canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                let moving = pane.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                    if phase != DispatchPhase::Capture {
                        return;
                    }
                    let taken = moving
                        .update(cx, |pane, cx| {
                            if pane.scrollbar.model.drag.is_none() {
                                return false;
                            }
                            pane.bar_move(
                                event.position,
                                event.pressed_button == Some(MouseButton::Left),
                                cx,
                            );
                            true
                        })
                        .unwrap_or(false);
                    if taken {
                        cx.stop_propagation();
                    }
                });
                let releasing = pane.clone();
                window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                    if phase != DispatchPhase::Capture || event.button != MouseButton::Left {
                        return;
                    }
                    let _ = releasing.update(cx, |pane, cx| {
                        pane.bar_release(cx);
                    });
                });
            },
        )
        .absolute()
        .size_full()
        .into_any_element()
    }

    pub(super) fn scrollbar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        self.fit_scrollbar();
        let (total, y) = {
            let paint = self.paint.borrow();
            (paint.total, paint.scroll_y)
        };
        if self
            .scrollbar
            .previous
            .is_some_and(|(count, previous)| count == total && previous != y)
        {
            self.reveal_scrollbar(cx);
        }
        self.scrollbar.previous = Some((total, y));
        let bar = &self.scrollbar;
        let height = f64::from(self.host.view.height);
        let content = total as f64 * f64::from(self.board().row_h);
        let overflow = self.host.view.opened && scroll::overflows(content, height);
        let wide = bar.model.lane == Some(Axis::Vertical) || bar.model.drag.is_some();
        let elapsed = bar.moved_at.map(|at| {
            cx.background_executor()
                .now()
                .saturating_duration_since(at)
                .as_millis() as u64
        });
        let opacity = if wide || elapsed.is_some_and(|ms| ms < scroll::HOLD_MS) {
            1.
        } else if cx.reduce_motion() {
            0.
        } else {
            elapsed.map_or(0., |ms| {
                1. - (ms.saturating_sub(scroll::HOLD_MS) as f32 / FADE_MS as f32).min(1.)
            })
        };
        if !wide
            && !cx.reduce_motion()
            && opacity > 0.
            && elapsed.is_some_and(|ms| ms >= scroll::HOLD_MS)
        {
            window.request_animation_frame();
        }
        let ink = self.paint.borrow().ink;
        let width = if wide {
            scroll::KNOB_WIDE_PX
        } else {
            scroll::KNOB_REST_PX
        };
        let length = scroll::handle_length(height, content, height);
        let offset = scroll::handle_offset(f64::from(y), content, height, height);
        let alpha = if bar.model.drag.is_some() {
            scroll::KNOB_PRESSED_ALPHA
        } else if wide {
            scroll::KNOB_HOVER_ALPHA
        } else {
            scroll::KNOB_ALPHA
        };
        let lane_w = self.board().pad_x;
        let mut lane = div().absolute().right_0().top_0().w(px(lane_w)).h_full();
        if overflow && opacity > 0. {
            if wide {
                lane = lane.bg(ink.foreground.opacity(scroll::TRACK_ALPHA * opacity));
            }
            lane = lane.child(
                div()
                    .debug_selector(|| "trash-scroll-knob".into())
                    .absolute()
                    .right(px(scroll::KNOB_INSET_PX as f32))
                    .top(px(offset as f32))
                    .w(px(width as f32))
                    .h(px(length as f32))
                    .rounded(px(
                        scroll::knob_radius(f64::from(f32::from(ink.radius)), width, length) as f32,
                    ))
                    .bg(ink.foreground.opacity(alpha * opacity)),
            );
        }
        lane.child(self.scrollbar_grab(cx)).into_any_element()
    }
}
