//! A platform scroll as the Settings pane's travel, which the Plugins page shares.
use gpui::ScrollDelta;

// The wheel as ui/js/Scroll.js moves the pane: a platform line is 24px times 4, so a three-line notch moves 288.
const LINE_TRAVEL: f32 = 24. * 4.;
// A touchpad moves 2.5 per raw pixel, and gpui-pre-linux hands raw travel over gained by three.
const TOUCH_TRAVEL: f32 = 2.5 / 3.;

/// A platform scroll as downward travel, a wheel line and a touchpad pixel each as far as the pane moves them.
pub(crate) fn travel(delta: ScrollDelta) -> f32 {
    match delta {
        ScrollDelta::Lines(lines) => -lines.y * LINE_TRAVEL,
        ScrollDelta::Pixels(pixels) => -f32::from(pixels.y) * TOUCH_TRAVEL,
    }
}
