//! The media surface's roles and sizes, one place to swap for the theme tokens when they land.
use crate::list::{Palette, Typeface};
use gpui::{Hsla, TextStyle};

/// Board roles: --bg, --surface, --fg, --muted, --muted-frame, --accent-frame, --hair and --error.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MediaInk {
    pub background: Hsla,
    pub surface: Hsla,
    pub foreground: Hsla,
    pub muted: Hsla,
    pub muted_frame: Hsla,
    pub accent_frame: Hsla,
    pub hair: Hsla,
    pub error: Hsla,
}

// The board's hair is the foreground at 12%.
const HAIR: f32 = 0.12;

impl MediaInk {
    /// Interim roles from the list's inherited palette until the theme tokens land.
    pub fn inherited(style: &TextStyle, theme: crate::palette::Theme) -> Self {
        let palette = Palette::inherited(style, theme);
        Self {
            background: palette.background,
            // MediaStrip's well is Theme.color.surface, the card ground, never the pane's (EY-429, EY-436).
            surface: theme.surface,
            foreground: palette.foreground,
            muted: palette.muted,
            muted_frame: palette.muted_frame,
            accent_frame: palette.accent_frame,
            hair: palette.foreground.opacity(HAIR),
            error: palette.error,
        }
    }
}

/// The theme's small body size, one under body as the text size moves: 13 at 14, 11 at 12.
pub fn body_small(typeface: &Typeface) -> f32 {
    f32::from(typeface.body) - 1.
}

/// Board sizes in pixels: ql-video.html, ql-audio.html and cols-preview-video.html.
pub mod size {
    /// Strip height, the chrome height.
    pub const STRIP: f32 = 27.;
    /// Gap between strip parts and the strip's side padding plus frame.
    pub const GAP: f32 = 9.;
    /// A control slot's width, the minimum hit size.
    pub const SLOT: f32 = 24.;
    /// Line heights the audio pane sets for name and facts, at body 14 and caption 12.
    pub const BODY_LINE: f32 = 18.;
    pub const CAPTION_LINE: f32 = 16.;
    /// The poster and audio marks.
    pub const STATE_MARK: f32 = 40.;
    /// The poster and audio marks' opacity over their ink.
    pub const STATE_MARK_OPACITY: f32 = 0.7;
    /// Gap between the audio mark and its text.
    pub const AUDIO_GAP: f32 = 14.;
    /// The row padding either side of the audio pane.
    pub const ROW_PAD: f32 = 14.;
    /// The audio pane's text column on the 902 card.
    pub const AUDIO_TEXT: f32 = 820.;
    /// Track and frame thickness.
    pub const TRACK: f32 = 2.;
    pub const HAIRLINE: f32 = 1.;
}
