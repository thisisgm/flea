//! A wheel notch's OutCubic glide: the scroll lands at once and paint follows from where it was drawn.
use super::{FocusFacts, GridView, Shown};
use crate::input::scroll::{out_cubic, RETURN_MS};
use std::time::Instant;

/// A wheel notch's glide, from the drawn position to the written one over the list's RETURN_MS.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glide {
    pub from: f32,
    pub to: f32,
    pub start: Instant,
    pub at: f32,
}

impl GridView {
    /// A wheel notch: the scroll lands at once and the paint glides there from where it was drawn, as List's notch does.
    pub fn wheel_by(
        &mut self,
        shown: Shown,
        focus: &dyn FocusFacts,
        delta: f32,
        glide_at: Option<Instant>,
    ) {
        let (from, before) = (self.drawn_scroll(), self.scroll);
        self.scroll_by(shown, focus, delta);
        if self.scroll != before {
            self.glide = glide_at.map(|start| Glide {
                from,
                to: self.scroll,
                start,
                at: from,
            });
        }
    }

    /// The position paint draws: the glide's while it heads for the current scroll, else the scroll.
    pub fn drawn_scroll(&self) -> f32 {
        self.glide
            .filter(|glide| glide.to == self.scroll)
            .map_or(self.scroll, |glide| glide.at)
    }

    /// One frame of the glide; false once it lands, or once another write moved the scroll.
    pub fn step_glide(&mut self, now: Instant) -> bool {
        let scroll = self.scroll;
        let Some(glide) = &mut self.glide else {
            return false;
        };
        let t = now.saturating_duration_since(glide.start).as_secs_f64() * 1000. / RETURN_MS;
        if glide.to != scroll || t >= 1. {
            self.glide = None;
            return false;
        }
        glide.at = glide.from + (glide.to - glide.from) * out_cubic(t) as f32;
        true
    }

    /// A press lands the glide where it is drawn, so the tile under the pointer is the one it hits.
    pub fn land_glide(&mut self) {
        if let Some(glide) = self.glide.take().filter(|glide| glide.to == self.scroll) {
            self.scroll = glide.at;
        }
    }
}
