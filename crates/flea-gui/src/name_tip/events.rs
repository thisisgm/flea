//! Key cancellation belongs to the focus root; paint-only deferred nodes do not receive its keys.
use super::{element::Source, Owner};
use gpui::{
    prelude::*, EntityId, MouseDownEvent, MouseExitEvent, MouseMoveEvent, MouseUpEvent,
    ScrollWheelEvent, Window,
};

pub(crate) fn key_events<E: InteractiveElement>(root: E, owner: Owner, entity: EntityId) -> E {
    let keyed = owner.clone();
    root.capture_key_down(move |_, _, cx| {
        if keyed.borrow_mut().dismiss() {
            cx.notify(entity);
        }
    })
    .on_modifiers_changed(move |_, _, cx| {
        if owner.borrow_mut().dismiss() {
            cx.notify(entity);
        }
    })
}

pub(super) fn listen(owner: &Owner, source: Source, entity: EntityId, window: &mut Window) {
    let moved = owner.clone();
    window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
        if !phase.capture() {
            return;
        }
        let pressed = event.pressed_button.is_some();
        {
            let owner = moved.borrow();
            if owner.pointer == Some(event.position) && owner.pressed == pressed {
                return;
            }
        }
        let next = if !pressed && window.is_window_active() && !cx.has_active_drag() {
            source(window, cx)
        } else {
            None
        };
        let eligible = next.is_some();
        let mut owner = moved.borrow_mut();
        let shown = owner.shaped.is_some();
        let changed = owner.pointer_moved(event.position, pressed, next);
        drop(owner);
        // A hidden view or a fitting row cannot invalidate the window merely because the pointer moves.
        if changed && (shown || eligible) {
            cx.notify(entity);
        }
    });
    let exited = owner.clone();
    window.on_mouse_event(move |_: &MouseExitEvent, phase, _, cx| {
        if phase.capture() && exited.borrow_mut().dismiss() {
            cx.notify(entity);
        }
    });
    let pressed = owner.clone();
    window.on_mouse_event(move |_: &MouseDownEvent, phase, _, cx| {
        if phase.capture() {
            let mut owner = pressed.borrow_mut();
            let shown = owner.dismiss();
            owner.pressed = true;
            if shown {
                cx.notify(entity);
            }
        }
    });
    let released = owner.clone();
    window.on_mouse_event(move |_: &MouseUpEvent, phase, _, _| {
        if phase.capture() {
            released.borrow_mut().pressed = false;
        }
    });
    let scrolled = owner.clone();
    window.on_mouse_event(move |_: &ScrollWheelEvent, phase, _, cx| {
        if phase.capture() && scrolled.borrow_mut().dismiss() {
            cx.notify(entity);
        }
    });
}
