//! Feedback surfaces: the empty hero, the pane state message and the status line's message lane, over core `ui_logic_status_feedback`.
pub(crate) mod brand;
pub mod caption;
#[cfg(test)]
mod caption_paint_tests;
#[cfg(test)]
mod caption_tests;
#[cfg(test)]
mod detail_tests;
pub(crate) mod empty_omi;
#[cfg(test)]
mod empty_omi_loop_tests;
#[cfg(test)]
mod empty_omi_tests;
#[cfg(test)]
mod fix9_tests;
pub mod hero;
#[cfg(test)]
mod hero_look_tests;
#[cfg(test)]
mod hero_tests;
pub mod line;
#[cfg(test)]
mod line_busy_tests;
#[cfg(test)]
mod line_tests;
#[cfg(test)]
mod loop_tests;
pub mod mark;
pub mod observe;
pub mod state;
#[cfg(test)]
mod state_tests;
#[cfg(test)]
mod test_root;

use crate::list::Palette;
use gpui::{App, Hsla};
use std::time::Instant;

/// The resolved palette roles these feedback surfaces draw in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ink {
    pub foreground: Hsla,
    pub metadata: Hsla,
    pub metadata_frame: Hsla,
    pub muted: Hsla,
    pub muted_frame: Hsla,
    pub error: Hsla,
}

impl Ink {
    /// The status bar stands on --surface (HIG list), so its error takes the red lifted on that ground.
    pub fn on_surface(palette: &Palette) -> Self {
        Self {
            error: palette.error_on_surface,
            ..Self::interim(palette)
        }
    }

    /// Maps the shared window palette to feedback roles.
    pub fn interim(palette: &Palette) -> Self {
        Self {
            foreground: palette.foreground,
            metadata: palette.metadata,
            metadata_frame: palette.metadata_frame,
            muted: palette.muted,
            muted_frame: palette.muted_frame,
            error: palette.error,
        }
    }
}

/// Milliseconds on the executor's clock since `base`, which a test advances by hand.
pub fn now_ms(base: Instant, cx: &App) -> u64 {
    cx.background_executor()
        .now()
        .saturating_duration_since(base)
        .as_millis() as u64
}

/// Counts a surface keeps so a test can prove it is idle at rest.
#[derive(Debug, Default)]
pub struct Counters {
    /// Renders of the surface.
    pub renders: std::cell::Cell<usize>,
    /// Animation frames it asked for.
    pub frames: std::cell::Cell<usize>,
    /// Timers it armed to wake at a deadline.
    pub timers: std::cell::Cell<usize>,
}

pub(crate) fn bump(cell: &std::cell::Cell<usize>) {
    cell.set(cell.get() + 1);
}
