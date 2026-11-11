//! Rail pointer input and the viewport Element.
use super::*;

const WHEEL_ROW_FRACTION: f32 = 0.78;

impl Render for Producer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.load(cx);
        // Read this frame's painted viewport after layout has published its rows.
        let producer = cx.entity().downgrade();
        cx.defer(move |cx| {
            let _ = producer.update(cx, |producer, cx| producer.inspect_visible(cx));
        });
        let model = self.model.borrow();
        self.ime_session = model.focused.then(|| model.editor.session()).flatten();
        self.ime_focus = self.ime_session.and_then(|_| window.focused(cx));
        drop(model);
        let element = RailElement::new(
            Rc::clone(&self.model),
            Rc::clone(&self.laid),
            self.typeface.clone(),
            cx.entity(),
        );
        div()
            .id("rail-producer")
            .size_full()
            .hover_listener_mode(gpui::HoverListenerMode::InputModalityIndependent)
            .on_hover(cx.listener(|producer, hovered: &bool, _, cx| {
                if *hovered {
                    return;
                }
                let mut model = producer.model.borrow_mut();
                let changed = model.hovered.is_some() || model.mark_hovered.is_some();
                model.hovered = None;
                model.mark_hovered = None;
                drop(model);
                if changed {
                    cx.notify();
                }
            }))
            .on_mouse_move(
                cx.listener(|producer, event: &gpui::MouseMoveEvent, _, cx| {
                    producer.editor_drag(event, cx);
                    producer.reorder_move(event, cx);
                    // A held drag is a platform drag in v0.3.12: no row hovers under it, and none stays lit after it leaves.
                    let held = cx.has_active_drag();
                    let row = producer.row_at(event.position).filter(|_| !held);
                    let mark = producer.mark_at(event.position).filter(|_| !held);
                    let mut model = producer.model.borrow_mut();
                    if model.hovered != row || model.mark_hovered != mark {
                        model.hovered = row;
                        model.mark_hovered = mark;
                        drop(model);
                        cx.notify();
                    }
                }),
            )
            .on_scroll_wheel(
                cx.listener(|producer, event: &gpui::ScrollWheelEvent, _, cx| {
                    let laid = producer.laid.borrow();
                    let limit = (laid.content_height - laid.height).max(0.);
                    let delta = f32::from(
                        event
                            .delta
                            .pixel_delta(gpui::px(
                                (crate::list::Metrics::for_typeface(&producer.typeface)
                                    .nominal_row
                                    * WHEEL_ROW_FRACTION)
                                    .round(),
                            ))
                            .y,
                    );
                    let mut model = producer.model.borrow_mut();
                    let before = model.scroll;
                    model.scroll = (model.scroll - delta).clamp(0., limit);
                    if model.scroll != before {
                        producer.inspected_visible.clear();
                    }
                    drop(model);
                    drop(laid);
                    producer.cancel_rename(cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|producer, event: &MouseDownEvent, _, cx| producer.press(event, cx)),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|producer, event: &MouseDownEvent, _, cx| producer.press(event, cx)),
            )
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(|producer, event: &MouseDownEvent, _, cx| producer.press(event, cx)),
            )
            .on_mouse_down_out(cx.listener(|producer, _: &MouseDownEvent, _, cx| {
                producer.cancel_rename(cx);
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|producer, event: &gpui::MouseUpEvent, _, cx| {
                    producer.editor_dragging = false;
                    producer.mark_release(event, cx);
                    producer.reorder_release(f32::from(event.position.y), cx);
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|producer, event: &gpui::MouseUpEvent, _, cx| {
                    producer.editor_dragging = false;
                    producer.mark_cancel();
                    // A drag released past the rail still lands; a press that never dragged opens nothing.
                    let dragged = producer
                        .model
                        .borrow()
                        .reorder
                        .is_some_and(|held| held.start_y.is_some());
                    if dragged {
                        producer.reorder_release(f32::from(event.position.y), cx);
                    } else {
                        producer.model.borrow_mut().reorder = None;
                    }
                }),
            )
            // RailDrop.qml: the rows receive this window's row drags; the window resolves the row and lands it.
            .on_drop::<crate::g2::drag_drop::join::Seed>({
                let sink = self.drop_sink.clone();
                move |_, window, cx| {
                    if let Some(sink) = &sink {
                        sink(window, cx);
                    }
                }
            })
            .child(element)
    }
}
