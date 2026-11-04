//! Pointer controls use the same selected application, checkbox and launch guard as the keyboard.
use super::*;
use shell::Control;

impl OpenWith {
    /// A moving pointer chooses its visible row without moving keyboard focus.
    pub fn hover(&mut self, at: usize) -> bool {
        if self.opening || at >= self.applications() || at == self.cursor {
            return false;
        }
        self.cursor = at;
        true
    }

    /// The shipped menu list folds raw notches or gained touchpad travel, with no momentum tail.
    pub fn scroll(&mut self, event: &gpui::ScrollWheelEvent, row_height: f32) {
        use crate::input::{scroll, wheel_delivery};
        use flea::ui_logic_mn1::fit;
        use gpui::TouchPhase;
        let in_stroke = self.stroke || event.touch_phase != TouchPhase::Moved;
        self.stroke = match event.touch_phase {
            TouchPhase::Started => true,
            TouchPhase::Ended | TouchPhase::Cancelled => false,
            TouchPhase::Moved => self.stroke,
        };
        let Some(wheel) = wheel_delivery(event, in_stroke) else {
            self.touch = 0.;
            return;
        };
        if self.opening || self.applications() == 0 {
            return;
        }
        let steps = if scroll::is_touchpad(wheel.phase) {
            let held = if wheel.phase == scroll::Phase::Begin {
                0.
            } else {
                self.touch
            };
            let (steps, rest) =
                fit::touch_steps(held, scroll::touch_distance(wheel.pixel) as f32, row_height);
            self.touch = rest;
            steps
        } else {
            let (steps, rest) = fit::notch_steps(self.notch, wheel.angle.round() as i32);
            self.notch = rest;
            steps
        };
        if steps != 0 {
            self.cursor = self
                .cursor
                .saturating_add_signed(steps as isize)
                .min(self.applications() - 1);
            self.follow();
        }
    }

    pub fn activate(&mut self, control: Control) -> Vec<Effect> {
        if self.opening && control != Control::Button(0) {
            return Vec::new();
        }
        match control {
            Control::Application(at) if at < self.applications() => {
                self.cursor = at;
                self.focus = Stop::List;
                self.launch()
            }
            Control::Check(0) => {
                self.focus = Stop::Always;
                self.key(Key::Space)
            }
            Control::Field => {
                self.focus = Stop::Search;
                Vec::new()
            }
            Control::Clear => {
                self.query.clear();
                self.focus = Stop::Search;
                self.refilter();
                Vec::new()
            }
            Control::Button(at) => self.press_at(at),
            _ => Vec::new(),
        }
    }
}
