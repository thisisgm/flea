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
    /// Until the theme tokens reach the GUI: the inherited palette's nearest roles and square corners.
    pub fn inherited(palette: &Palette) -> Self {
        Self {
            background: palette.background,
            surface: palette.background,
            foreground: palette.foreground,
            bright: palette.foreground,
            muted: palette.metadata,
            meta: palette.metadata,
            muted_frame: palette.metadata_frame,
            error: palette.foreground,
            green: palette.foreground,
            cyan: palette.foreground,
            corner: px(0.),
            control_corner: px(0.),
        }
    }

    /// The inherited roles of a window's text style, for a host with no theme tokens yet.
    pub fn of_text(style: &gpui::TextStyle) -> Self {
        Self::inherited(&Palette::inherited(style))
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

/// Quick Look's bar: 27 px, 14 px padding, 9 px gaps, 12 px caption, 16 px marks.
pub const BAR: f32 = 27.;
pub const PAD: f32 = 14.;
pub const GAP: f32 = 9.;
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
