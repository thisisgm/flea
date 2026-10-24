//! Rail pointer input and the viewport Element.
use super::*;

const WHEEL_ROW_FRACTION: f32 = 0.78;

impl Render for Producer {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.load(cx);
        let element = RailElement::new(
            Rc::clone(&self.model),
            Rc::clone(&self.laid),
            self.typeface.clone(),
            cx.entity(),
        );
        div()
            .size_full()
            .on_mouse_move(
                cx.listener(|producer, event: &gpui::MouseMoveEvent, _, cx| {
                    producer.editor_drag(event, cx);
                    let row = producer.row_at(event.position);
                    if producer.model.borrow().hovered != row {
                        producer.model.borrow_mut().hovered = row;
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
                    model.scroll = (model.scroll - delta).clamp(0., limit);
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
                cx.listener(|producer, _, _, _| {
                    producer.editor_dragging = false;
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|producer, _, _, _| {
                    producer.editor_dragging = false;
                }),
            )
            .child(element)
    }
}
