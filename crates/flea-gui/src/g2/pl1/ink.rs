//! The card roles the unlock board draws with, filled from the inherited style until G0.5's theme tokens land.
use crate::list::Palette;
use gpui::Hsla;

#[derive(Clone, Copy, Debug)]
pub struct CardInk {
    pub surface: Hsla,
    pub ground: Hsla,
    pub foreground: Hsla,
    pub muted: Hsla,
    pub frame: Hsla,
    pub accent_frame: Hsla,
    pub error: Hsla,
    pub rule: Hsla,
    pub wash: Hsla,
}

impl CardInk {
    /// Interim roles from the row palette; error and accent stand in as foreground until the tokens arrive.
    pub fn inherited(palette: &Palette) -> Self {
        Self {
            surface: palette.background,
            ground: palette.background,
            foreground: palette.foreground,
            muted: palette.metadata,
            frame: palette.metadata_frame,
            accent_frame: palette.accent_frame,
            error: palette.foreground,
            rule: palette.metadata_frame.opacity(0.4),
            wash: palette.accent.opacity(0.14),
        }
    }
}
