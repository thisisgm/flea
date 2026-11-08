//! A flyout row still waiting on its answer (AirDrop's Looking for devices…): Omi thinking in a 22px slot, in the accent.
use crate::feedback::spinner::{device_side, paint_rects, request_frame, Mode, Motion};
use gpui::{App, Bounds, Hsla, Pixels, Window};
use std::cell::RefCell;

/// The OMI-SPINNER small slot the board draws in a menu row.
pub const SIDE: f32 = 22.;

thread_local! {
    // One player for every busy row: a menu holds at most one, and its phase carries across repaints.
    static MOTION: RefCell<Option<Motion>> = const { RefCell::new(None) };
}

pub fn paint(slot: Bounds<Pixels>, ink: Hsla, window: &mut Window, cx: &mut App) {
    let moving = !cx.reduce_motion();
    let side =
        device_side(f32::from(slot.size.width.min(slot.size.height)) * window.scale_factor());
    let now = cx.background_executor().now();
    MOTION.with_borrow_mut(|motion| {
        let motion = motion.get_or_insert_with(|| Motion::new(Mode::Thinking));
        motion.sample(moving, None, side, now);
        paint_rects(motion, slot, ink, window);
    });
    if moving {
        request_frame(window, cx);
    }
}
