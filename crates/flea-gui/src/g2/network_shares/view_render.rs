//! The card takes pointer and keyboard input only while open.
use super::{drawing::NetworkElement, view::NetworkView};
use gpui::{div, prelude::*, px, Context, MouseButton, Render, Window};

impl Render for NetworkView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dimensions = (
            f32::from(window.viewport_size().width),
            f32::from(window.viewport_size().height),
        );
        if dimensions != self.dimensions {
            self.dimensions = dimensions;
            self.fit_focus();
            self.revision += 1;
        }
        let motion = self.motion.sample(
            self.model.opened,
            cx.background_executor().now(),
            cx.reduce_motion(),
        );
        if motion.animating {
            window.request_animation_frame();
        }
        let root = div()
            .id("g2-pl2-network")
            .size_full()
            .relative()
            .opacity(motion.opacity)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key));
        if !motion.visible {
            return root;
        }
        let input = div()
            .debug_selector(|| "network-motion".into())
            .size_full()
            .relative()
            .top(px(motion.rise))
            .child(NetworkElement(cx.entity()))
            .when(self.model.opened, |input| {
                input
                    .on_scroll_wheel(cx.listener(|this, event: &gpui::ScrollWheelEvent, _, cx| {
                        cx.stop_propagation();
                        let height = this.body_height(this.dimensions.1);
                        this.scroll = (this.scroll - f32::from(event.delta.pixel_delta(px(37.)).y))
                            .clamp(0., (this.content_height() - height).max(0.));
                        this.changed(cx);
                    }))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            let drawn = this.drawn.borrow();
                            let (x, y) = (
                                f32::from(event.position.x) - drawn.origin.0,
                                f32::from(event.position.y) - drawn.origin.1,
                            );
                            let outside = !drawn.card.is_some_and(|r| r.contains(x, y));
                            drop(drawn);
                            if outside {
                                this.close(cx);
                            }
                        }),
                    )
                    .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                        if this.text_drag(event.position, cx) {
                            return;
                        }
                        let (x, y) = (f32::from(event.position.x), f32::from(event.position.y));
                        let drawn = this.drawn.borrow();
                        let hover = drawn
                            .hits
                            .iter()
                            .rev()
                            .find(|(_, rect)| rect.contains(x - drawn.origin.0, y - drawn.origin.1))
                            .map(|(focus, _)| *focus);
                        drop(drawn);
                        if hover != this.hovered {
                            this.hovered = hover;
                            this.paint_changed(cx);
                        } else {
                            cx.notify();
                        }
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            let (x, y) = (f32::from(event.position.x), f32::from(event.position.y));
                            let drawn = this.drawn.borrow();
                            let (x, y) = (x - drawn.origin.0, y - drawn.origin.1);
                            let focus = drawn
                                .hits
                                .iter()
                                .rev()
                                .find(|(_, rect)| rect.contains(x, y))
                                .map(|(focus, _)| *focus);
                            let outside = !drawn.card.is_some_and(|r| r.contains(x, y));
                            drop(drawn);
                            if outside {
                                this.close(cx);
                                return;
                            }
                            if focus.is_some_and(|focus| {
                                this.text_down(focus, event.position, event.modifiers.shift, cx)
                            }) {
                                this.pressed = None;
                                return;
                            }
                            this.pressed = focus;
                            if focus == Some(super::dialog::Focus::Eye) {
                                this.reveal_held(true, cx);
                            }
                            this.paint_changed(cx);
                        }),
                    )
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, event: &gpui::MouseUpEvent, _, cx| {
                            if this.selecting.take().is_some() {
                                this.paint_changed(cx);
                                return;
                            }
                            let drawn = this.drawn.borrow();
                            let (x, y) = (
                                f32::from(event.position.x) - drawn.origin.0,
                                f32::from(event.position.y) - drawn.origin.1,
                            );
                            let focus = drawn
                                .hits
                                .iter()
                                .rev()
                                .find(|(_, rect)| rect.contains(x, y))
                                .map(|(focus, _)| *focus);
                            drop(drawn);
                            this.reveal_held(false, cx);
                            if let Some(focus) = focus.filter(|f| Some(*f) == this.pressed.take()) {
                                this.pointer_activate(focus, cx);
                            }
                            this.pressed = None;
                            this.paint_changed(cx);
                        }),
                    )
                    .on_mouse_up_out(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.pressed = None;
                            this.selecting = None;
                            this.reveal_held(false, cx);
                        }),
                    )
            });
        root.child(crate::g2::file_ops::view::scrim(
            &self.palette,
            self.cards_below,
            input,
        ))
    }
}
