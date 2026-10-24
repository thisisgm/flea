use super::MARK_FILL;
use crate::{palette::Theme, row::NameInk};
use gpui::{Hsla, TextStyle};

/// The palette roles a row draws with, to be filled from Omarchy's theme.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub background: Hsla,
    pub foreground: Hsla,
    pub metadata: Hsla,
    pub metadata_frame: Hsla,
    pub accent: Hsla,
    pub accent_frame: Hsla,
    pub symlink: Hsla,
    pub executable: Hsla,
    pub marked: Hsla,
    pub muted_frame: Hsla,
}

impl Palette {
    /// An inherited background keeps its own text colour and derived inks; otherwise Omarchy's theme sets every role.
    pub fn inherited(style: &TextStyle, theme: Theme) -> Self {
        let Some(background) = style.background_color else {
            return Self {
                background: theme.background,
                foreground: theme.foreground,
                metadata: theme.metadata,
                metadata_frame: theme.metadata_frame,
                accent: theme.accent,
                accent_frame: theme.accent_frame,
                symlink: theme.symlink,
                executable: theme.executable,
                marked: theme.foreground.opacity(MARK_FILL),
                muted_frame: theme.muted_frame,
            };
        };
        let foreground = style.color;
        Self {
            background,
            foreground,
            metadata: foreground.opacity(0.62),
            metadata_frame: foreground.opacity(0.5),
            accent: theme.accent,
            accent_frame: theme.accent_frame,
            symlink: foreground,
            executable: foreground,
            marked: foreground.opacity(MARK_FILL),
            muted_frame: foreground.opacity(0.5),
        }
    }

    pub fn name(&self, ink: NameInk, lifted: bool) -> Hsla {
        match (lifted, ink) {
            (true, _) | (false, NameInk::Foreground) => self.foreground,
            (false, NameInk::Symlink) => self.symlink,
            (false, NameInk::Executable) => self.executable,
        }
    }
}
