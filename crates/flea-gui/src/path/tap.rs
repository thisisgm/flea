//! Toolbar taps use the oracle's platform drag distance and permanently cancel on a drag.
use super::chrome::Frame;
use crate::columns::preview_host::START_DRAG_DISTANCE;
use gpui::{DispatchPhase, MouseButton, MouseMoveEvent, Pixels, Point, Window};
use std::{cell::RefCell, rc::Rc};

/// QStyleHints::mousePressAndHoldInterval: a TapHandler press held this long is a long press, never a tap (EY-200).
pub const LONG_PRESS: std::time::Duration = std::time::Duration::from_millis(800);

/// A release this soon after its press is still a tap.
pub fn short_press(pressed_at: Option<std::time::Instant>, now: std::time::Instant) -> bool {
    pressed_at.is_some_and(|at| now.saturating_duration_since(at) < LONG_PRESS)
}

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
