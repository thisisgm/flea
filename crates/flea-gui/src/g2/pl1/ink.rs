//! The card roles the unlock board draws with, filled from the inherited style until G0.5's theme tokens land.
use crate::list::Palette;
use gpui::Hsla;

#[derive(Clone, Copy, Debug)]
pub struct CardInk {
    pub radius: f32,
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
    /// Reads the resolved row palette, including independent muted, frame and error roles.
    pub fn inherited(palette: &Palette) -> Self {
        Self {
            radius: crate::compositor::CORNER_DEFAULT,
            surface: palette.background,
            ground: palette.background,
            foreground: palette.foreground,
            muted: palette.muted,
            frame: palette.muted_frame,
            accent_frame: palette.accent_frame,
            error: palette.error,
            rule: palette.muted_frame.opacity(0.4),
            wash: palette.accent.opacity(0.14),
        }
    }
}
