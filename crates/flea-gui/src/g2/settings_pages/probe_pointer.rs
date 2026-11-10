//! Passive pointer facts for the same clipped boxes the component laid out.
use gpui::{
    Bounds, DispatchPhase, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    Window,
};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

pub(super) type Pointer = Rc<RefCell<BTreeMap<String, (bool, bool)>>>;

pub(super) fn listen(pointer: Pointer, id: String, bounds: Bounds<Pixels>, window: &mut Window) {
    pointer.borrow_mut().entry(id.clone()).or_default().0 =
        bounds.contains(&window.mouse_position());
    let (moved, down, up) = (pointer.clone(), pointer.clone(), pointer);
    let (a, b) = (id.clone(), id.clone());
    window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, _| {
        if phase == DispatchPhase::Capture {
            moved.borrow_mut().entry(a.clone()).or_default().0 = bounds.contains(&event.position);
        }
    });
    window.on_mouse_event(move |event: &MouseDownEvent, phase, _, _| {
        if phase == DispatchPhase::Capture && event.button == MouseButton::Left {
            down.borrow_mut().entry(b.clone()).or_default().1 = bounds.contains(&event.position);
        }
    });
    window.on_mouse_event(move |event: &MouseUpEvent, phase, _, _| {
        if phase == DispatchPhase::Capture && event.button == MouseButton::Left {
            up.borrow_mut().entry(id.clone()).or_default().1 = false;
        }
    });
}
