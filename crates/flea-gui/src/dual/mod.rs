//! Dual layout, pane focus, divider interaction and session persistence.
pub mod layout;
pub mod session;

#[cfg(test)]
#[path = "divider_tests.rs"]
mod divider_tests;

pub use layout::{
    line_offsets, share_at, share_of, split_of, Drag, EQUAL_SPLIT, ROOM_MIN, SCALE, ZONE,
};
pub use session::{pane_focused, remember_patch, split_patch, stored_split};

use crate::list::Metrics;
use gpui::Pixels;

impl Metrics {
    /// The Dual mark box: 19px slot and 18px glyph at body 14, in place of the list's 23px slot and 17px glyph.
    pub fn for_dual(mut self, dual: bool, body: Pixels) -> Self {
        if dual {
            let body = f32::from(body);
            self.slot = (19. * body / 14.).round();
            self.mark = (18. * body / 14.).round();
        }
        self
    }
}

mod element;
pub mod focus;
mod selection;
mod window;

#[cfg(test)]
mod focus_tests;

mod activation;
#[cfg(test)]
mod activation_tests;

#[cfg(test)]
mod review_tests;

#[cfg(test)]
mod native_reset_tests;
#[cfg(test)]
mod pointer_tests;
