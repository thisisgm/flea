//! Menu ink roles shared by every card.
use super::{RULE, WASH};
use crate::list::Palette;
use gpui::Hsla;

/// The roles a menu paints with, read from the pane's palette until the theme tokens land.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MenuInk {
    pub surface: Hsla,
    pub frame: Hsla,
    pub rule: Hsla,
    pub wash: Hsla,
    pub label: Hsla,
    pub accent: Hsla,
    pub muted: Hsla,
    pub mark: Hsla,
    pub error: Hsla,
    pub tags: [Hsla; 7],
}

impl MenuInk {
    pub fn from_palette(palette: &Palette) -> Self {
        let frames = crate::presentation::frames::Frames::from_palette(palette, 0.);
        Self {
            surface: palette.background,
            frame: frames.ink,
            rule: frames.ink.opacity(RULE),
            wash: palette.foreground.opacity(WASH),
            label: palette.foreground,
            accent: palette.accent,
            muted: palette.muted,
            mark: frames.ink,
            error: palette.error,
            tags: [palette.foreground; 7],
        }
    }
}
