//! Ctrl and the wheel or touchpad walk the thumbnail stops, as GridArea.qml zoomWheel does.
use super::{geometry::ThumbSize, FocusFacts, GridView, Shown};

/// One wheel event under Ctrl: touchpad pixels count when a stroke is in phase or carries no angle, else wheel units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZoomInput {
    pub touchpad: bool,
    pub pixels: f32,
    pub angle: f32,
}

/// Scroll.js zoomTravel with GridArea.qml's 48 px touchpad step and 120 unit notch.
pub fn zoom_travel(input: ZoomInput) -> f32 {
    if input.pixels != 0. && (input.touchpad || input.angle == 0.) {
        input.pixels / 48.
    } else {
        input.angle / 120.
    }
}

impl GridView {
    /// GridArea.qml zoomWheel: travel accrues and whole steps walk the thumbnail stops; the answer is the size to persist as preview.thumbSize.
    pub fn zoom(
        &mut self,
        shown: Shown,
        focus: &dyn FocusFacts,
        input: ZoomInput,
    ) -> Option<ThumbSize> {
        self.zoom_travel += zoom_travel(input);
        let steps = self.zoom_travel.trunc();
        if steps == 0. {
            return None;
        }
        self.zoom_travel -= steps;
        let next = self.thumb.stepped(steps as i32);
        if next == self.thumb {
            return None;
        }
        self.reflow(shown, focus, |grid| grid.thumb = next);
        Some(next)
    }
}
