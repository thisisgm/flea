//! The roles a card draws with, taken from the window's palette until the theme tokens land.
use crate::list::Palette;
use flea::ui_logic_op1::card::Ink;
use gpui::Hsla;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CardPalette {
    pub background: Hsla,
    pub surface: Hsla,
    pub foreground: Hsla,
    pub muted: Hsla,
    pub muted_frame: Hsla,
    pub accent: Hsla,
    pub accent_frame: Hsla,
    // Seam: the theme's error role; until it lands the foreground stands in, so nothing invents a red.
    pub error: Hsla,
    pub error_on_surface: Hsla,
    pub radius: f32,
}

impl CardPalette {
    pub fn from_list(palette: &Palette, radius: f32) -> Self {
        Self {
            background: palette.background,
            surface: palette.background,
            foreground: palette.foreground,
            muted: palette.metadata,
            muted_frame: palette.metadata_frame,
            accent: palette.accent,
            accent_frame: palette.accent_frame,
            error: palette.foreground,
            error_on_surface: palette.foreground,
            radius,
        }
    }

    pub fn ink(&self, ink: Ink) -> Option<Hsla> {
        match ink {
            Ink::MutedFrame => Some(self.muted_frame),
            Ink::Accent => Some(self.accent),
            Ink::AccentFrame => Some(self.accent_frame),
            Ink::Foreground => Some(self.foreground),
            Ink::Error => Some(self.error),
            Ink::ErrorOnSurface => Some(self.error_on_surface),
            Ink::None => None,
        }
    }
}
