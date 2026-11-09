//! pv1's theme roles and the boards' metrics: every colour Quick Look and the preview column draw comes from here.
use crate::list::Palette;
use gpui::{px, Hsla, Pixels};

/// The board roles pv1 draws with; the host fills them from the theme tokens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    pub background: Hsla,
    pub surface: Hsla,
    pub foreground: Hsla,
    pub bright: Hsla,
    pub muted: Hsla,
    pub meta: Hsla,
    pub muted_frame: Hsla,
    pub error: Hsla,
    pub green: Hsla,
    pub cyan: Hsla,
    /// var(--r), the compositor's window rounding.
    pub corner: Pixels,
    /// var(--cr), a control's rounding.
    pub control_corner: Pixels,
}

impl Look {
    /// Frame ink comes from the shared presentation roles; the host supplies live geometry.
    pub fn inherited(palette: &Palette) -> Self {
        Self {
            background: palette.background,
            surface: palette.background,
            foreground: palette.foreground,
            bright: palette.foreground,
            muted: palette.muted,
            meta: palette.metadata,
            muted_frame: crate::presentation::frames::Frames::from_palette(palette, 0.).ink,
            error: palette.error,
            green: palette.foreground,
            cyan: palette.foreground,
            corner: px(0.),
            control_corner: px(0.),
        }
    }

    /// Inherited roles of a window's text style: the window's bare ground, the theme surface on a framed card.
    pub fn of_text(style: &gpui::TextStyle, theme: crate::palette::Theme) -> Self {
        let mut look = Self::inherited(&Palette::inherited(style, theme));
        look.surface = theme.surface;
        // CompareView.qml inkOf: an added row in the code inks' green, a hunk header in their cyan.
        look.green = theme.code_inks[1];
        look.cyan = theme.code_inks[5];
        look
    }

    pub fn with_frames(mut self, frames: crate::presentation::frames::Frames) -> Self {
        self.muted_frame = frames.ink;
        self.corner = px(frames.radius);
        self.control_corner = self.corner;
        self
    }

    /// var(--hair): the foreground at 12%.
    pub fn hair(&self) -> Hsla {
        self.foreground.opacity(HAIR)
    }

    /// var(--scrim): the background at 50%.
    pub fn scrim(&self) -> Hsla {
        self.background.opacity(SCRIM)
    }

    /// var(--hover), the pointer's wash, in an ink.
    pub fn wash(ink: Hsla, pressed: bool) -> Hsla {
        ink.opacity(if pressed { WASH_PRESS } else { WASH_HOVER })
    }
}

const HAIR: f32 = 0.12;
const SCRIM: f32 = 0.5;
const WASH_HOVER: f32 = 0.08;
const WASH_PRESS: f32 = 0.14;
/// Buttons.js DISABLED_OPACITY.
pub const DISABLED: f32 = 0.55;
/// Buttons.js PRESS_SCALE.
pub const PRESS_SCALE: f32 = 0.96;

/// Quick Look's bar: 27 px, 14 px padding, 9 px gaps, 14 px between view words, 12 px caption, 16 px marks.
pub const BAR: f32 = 27.;
pub const PAD: f32 = 14.;
pub const GAP: f32 = 9.;
pub const WORD_GAP: f32 = 14.;
pub const CAPTION: f32 = 12.;
pub const BODY: f32 = 14.;
pub const CHROME_MARK: f32 = 16.;
/// A chrome control's slot, and the focus ring inside it.
pub const SLOT_W: f32 = 24.;
pub const SLOT_H: f32 = 26.;
pub const RING: f32 = 20.;
/// Compare's refusal bar is a pixel shorter, with 13 px padding and a 24 by 25 close slot.
pub const REFUSAL_BAR: f32 = 26.;
pub const REFUSAL_PAD: f32 = 13.;
pub const REFUSAL_SLOT_H: f32 = 25.;
/// The can't-show recipe: a 40 px mark, the 14 px sentence, 9 px between parts.
pub const MARK: f32 = 40.;
/// A framed button: 30 px tall, 9 px padding.
pub const BUTTON_H: f32 = 30.;
/// Text lines: 20 px rows from 41 px down, 31 px in and 20 px clear of the right edge.
pub const TEXT_TOP: f32 = 41.;
pub const TEXT_LINE: f32 = 20.;
pub const TEXT_LEFT: f32 = 31.;
pub const TEXT_RIGHT: f32 = 20.;
/// The text viewport (PreviewText's CodeLines): from 41 px down to 14 px over the bottom, 14 px in at each side.
pub const TEXT_VIEW_BOTTOM: f32 = 14.;
/// Compare rows: a viewport under the 26 px bar with 12 px margins, 22 px rows, two 33 px number columns, the marker and the text.
pub const DIFF_VIEW_TOP: f32 = 26.;
pub const DIFF_MARGIN: f32 = 12.;
/// CompareView's contentPadding after the longest line.
pub const DIFF_END_PAD: f32 = 16.;
pub const DIFF_LINE: f32 = 22.;
pub const DIFF_OLD: f32 = 0.;
pub const DIFF_NEW: f32 = 44.;
pub const DIFF_NUMBER_W: f32 = 33.;
pub const DIFF_MARKER: f32 = 87.;
pub const DIFF_TEXT: f32 = 106.;
/// The preview column: 14 px inset, a 173 px frame, a 20 px name, 71 px labels on 18 px rows 3 px apart.
pub const COLUMN_INSET: f32 = 14.;
pub const COLUMN_FRAME: f32 = 173.;
pub const COLUMN_NAME: f32 = 20.;
pub const FACT_LABEL_W: f32 = 71.;
pub const FACT_ROW: f32 = 18.;
pub const FACT_ROW_GAP: f32 = 3.;
pub const FACT_LINE: f32 = 16.;
/// KindStack.qml: each mark behind the front one sits a quarter mark left, an eleventh down and 0.3 fainter.
pub const STACK_STEP_X: f32 = 10.;
pub const STACK_STEP_Y: f32 = 4.;
pub const STACK_FADE: f32 = 0.3;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::list::Palette;

    /// ql-error: the warning triangle over "This file could not be read." wears the theme's error ink, not the foreground.
    #[test]
    fn the_error_role_is_the_palettes_error_ink() {
        let ink = |value: u32| -> Hsla { gpui::rgb(value).into() };
        let palette = Palette {
            background: ink(0x101315),
            foreground: ink(0xcacccc),
            muted: ink(0x808080),
            error: ink(0xf38ba8),
            error_on_surface: ink(0xd20f39),
            metadata: ink(0x808080),
            metadata_frame: ink(0x808080),
            accent: ink(0x509475),
            accent_frame: ink(0x509475),
            symlink: ink(0x808080),
            executable: ink(0x808080),
            marked: ink(0x808080),
            muted_frame: ink(0x808080),
            tags: [ink(0x808080); 7],
        };
        assert_eq!(Look::inherited(&palette).error, palette.error);
    }
    /// EY-404, EY-408: QuickLookBar.qml's count and quiet words and FactsTable.qml's names take Theme.color.muted.
    #[test]
    fn ey404_the_muted_role_is_the_themes_muted_caption() {
        use crate::palette::Theme;
        let latte = "background='#eff1f5'\nforeground='#4c4f69'\naccent='#1e66f5'\ndark_background='#e3e4e8'\nmuted='#acb0be'";
        let sunset = "background='#1c120d'\nforeground='#c6b395'\naccent='#d28a41'\ndark_background='#150d0a'\nmuted='#68544b'";
        for (name, text) in [("catppuccin-latte", latte), ("sunset", sunset)] {
            let theme = Theme::from_text(text);
            let look = Look::of_text(&gpui::TextStyle::default(), theme);
            assert_ne!(
                theme.muted, theme.metadata,
                "{name}: the fixture needs two inks"
            );
            assert_eq!(look.muted, theme.muted, "{name}: muted");
            assert_eq!(
                look.meta, theme.metadata,
                "{name}: meta keeps the metadata ink"
            );
        }
        let look = Look::of_text(&gpui::TextStyle::default(), Theme::from_text(latte));
        assert!(
            look.muted.l > look.foreground.l,
            "on a light theme the muted words read lighter than the name"
        );
    }
    /// EY-487: CompareView.qml's inkOf draws an added row in the code inks' green and a hunk header in their cyan, on every theme.
    #[test]
    fn fixpreview_ey487_compare_added_and_hunk_take_the_code_inks() {
        use crate::palette::Theme;
        let latte = "background='#eff1f5'\nforeground='#4c4f69'\naccent='#1e66f5'\ndark_background='#e3e4e8'\ngreen='#40a02b'\ncyan='#179299'";
        let sunset = "background='#1c120d'\nforeground='#c6b395'\naccent='#d28a41'\ndark_background='#150d0a'\ncolor2='#799a55'\ncolor6='#599181'";
        for (name, text) in [("catppuccin-latte", latte), ("sunset", sunset)] {
            let theme = Theme::from_text(text);
            let look = Look::of_text(&gpui::TextStyle::default(), theme);
            assert_eq!(look.green, theme.code_inks[1], "{name}: added rows");
            assert_eq!(look.cyan, theme.code_inks[5], "{name}: the hunk header");
            // The themes' green sits near 89 to 109 degrees and their cyan near 163 to 183.
            let degrees = |ink: gpui::Hsla| ink.h * 360.;
            assert!(
                (70. ..140.).contains(&degrees(look.green)),
                "{name}: added rows draw green"
            );
            assert!(
                (150. ..200.).contains(&degrees(look.cyan)),
                "{name}: the hunk header draws cyan"
            );
            assert_ne!(
                look.green, look.foreground,
                "{name}: added is not the context ink"
            );
            assert_ne!(
                look.cyan, look.foreground,
                "{name}: the hunk is not the context ink"
            );
            assert_eq!(
                look.error, theme.error,
                "{name}: removed rows keep the error ink"
            );
        }
    }
}
