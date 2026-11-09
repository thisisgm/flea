//! The roles a card draws with, taken from the immutable window theme.
use crate::list::Palette;
use flea::ui_logic_file_ops::card::Ink;
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
    // Theme.qml lifts the palette red separately against the listing and card grounds.
    pub error: Hsla,
    pub error_on_surface: Hsla,
    pub radius: f32,
}

impl CardPalette {
    pub fn from_theme(style: &gpui::TextStyle, theme: crate::palette::Theme, radius: f32) -> Self {
        Self {
            surface: theme.surface,
            muted: theme.muted,
            muted_frame: theme.muted_frame,
            error: theme.error,
            error_on_surface: theme.error_on_surface,
            accent: theme.accent,
            accent_frame: theme.accent_frame,
            ..Self::from_list(&Palette::inherited(style, theme), radius)
        }
    }

    pub fn from_list(palette: &Palette, radius: f32) -> Self {
        let frames = crate::presentation::frames::Frames::from_palette(palette, radius);
        Self {
            background: palette.background,
            surface: palette.background,
            foreground: palette.foreground,
            muted: palette.muted,
            muted_frame: frames.ink,
            accent: palette.accent,
            accent_frame: palette.accent_frame,
            error: palette.error,
            error_on_surface: palette.error_on_surface,
            radius: frames.radius,
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
