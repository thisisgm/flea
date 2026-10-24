//! Toolbar taps use the oracle's platform drag distance and permanently cancel on a drag.
use super::chrome::Frame;
use crate::columns::preview_host::START_DRAG_DISTANCE;
use gpui::{DispatchPhase, MouseButton, MouseMoveEvent, Pixels, Point, Window};
use std::{cell::RefCell, rc::Rc};

/// Qt tests each axis against the original scene press position, with equality still a tap.
pub fn within_threshold(origin: Option<Point<Pixels>>, position: Point<Pixels>) -> bool {
    origin.is_some_and(|origin| {
        f32::from(position.x - origin.x).abs() <= START_DRAG_DISTANCE
            && f32::from(position.y - origin.y).abs() <= START_DRAG_DISTANCE
    })
}

/// Capture moves throughout the window so another hit box cannot hide a threshold crossing.
pub fn observe(frame: Rc<RefCell<Frame>>, window: &mut Window) {
    window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
        if phase != DispatchPhase::Capture {
            return;
        }
        let mut changed = false;
        for control in &mut frame.borrow_mut().controls {
            if control.pointer_down
                && (event.pressed_button != Some(MouseButton::Left)
                    || !within_threshold(control.press_origin, event.position)
                    || !control
                        .bounds
                        .is_some_and(|bounds| bounds.contains(&event.position)))
            {
                control.set_pressed(false, cx.background_executor().now(), cx.reduce_motion());
                changed = true;
            }
        }
        if changed {
            window.refresh();
        }
    });
}
