// One card rhythm for every dialog, the scrim under it, and the inks a button takes in each state.

pub const PADDING: f32 = 16.0;
pub const GAP: f32 = 12.0;
// The air above the button row: the part gap plus the row's own margin, 22 in all.
pub const BUTTONS_MARGIN: f32 = 10.0;
pub const TITLE_SIZE: f32 = 16.0;
// The boards' base line height, 1.2 of the font size, which the title row and every unstyled line take.
pub const LINE: f32 = 1.2;
pub const TITLE_LINE: f32 = TITLE_SIZE * LINE;
pub const FIELD_LABEL_LINE: f32 = 14.0;
pub const FIELD_LABEL_GAP: f32 = 5.0;
// The field name's letter spacing, added after every letter as CSS letter-spacing does.
pub const FIELD_LABEL_TRACK: f32 = 1.2;
pub const FIELD_GAP: f32 = 8.0;
pub const FIELD_SELECTION: f32 = 20.0;
pub const CARET_HEIGHT: f32 = 16.0;
pub const ESC_SIZE: f32 = 12.0;
pub const BODY_SIZE: f32 = 14.0;
pub const BODY_LINE: f32 = 20.0;
pub const EXPLAIN_SIZE: f32 = 12.0;
pub const EXPLAIN_LINE: f32 = 18.0;
pub const RULE_ALPHA: f32 = 0.40;
pub const SCRIM_ALPHA: f32 = 0.50;
pub const BUTTON_HEIGHT: f32 = 30.0;
pub const BUTTON_PAD_X: f32 = 9.0;
pub const BUTTON_GAP: f32 = 9.0;
pub const PRIMARY_WASH: f32 = 0.14;
pub const HOVER_WASH: f32 = 0.08;
pub const PRESS_WASH: f32 = 0.14;
pub const PRESS_SCALE: f32 = 0.96;
pub const DISABLED_ALPHA: f32 = 0.55;
pub const PRESS_MS: u64 = 150;
pub const NAME_MARK: f32 = 19.0;
pub const NAME_GAP: f32 = 9.0;
pub const FIELD_WASH: f32 = 0.24;

// The three widths the boards draw: the delete card, the collision card, Copy To. New File takes Copy To's width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Delete,
    Collide,
    CopyTo,
    NewFile,
}

pub fn width(kind: Kind) -> f32 {
    match kind {
        Kind::Delete => 416.0,
        Kind::Collide => 448.0,
        Kind::CopyTo | Kind::NewFile => 552.0,
    }
}

// The card never runs wider than the window less a padding on each side.
pub fn fitted_width(kind: Kind, window: f32) -> f32 {
    width(kind).min((window - 2.0 * PADDING).max(0.0))
}

// Title and buttons stay pinned; only the body between them scrolls when the window is short.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub height: f32,
    pub body: f32,
    pub scrolls: bool,
}

pub const RULE: f32 = 1.0;
// The card's frame; the padding sits inside it, as the boards' CSS boxes draw.
pub const BORDER: f32 = 1.0;

// Sample input: frame(20, 300, 700) for a header line of 20, a body of 300, a window 700 tall.
pub fn frame(title: f32, body: f32, window: f32) -> Frame {
    let chrome = 2.0 * BORDER
        + PADDING
        + title
        + GAP
        + RULE
        + GAP
        + GAP
        + BUTTONS_MARGIN
        + BUTTON_HEIGHT
        + PADDING;
    let room = (window - 2.0 * PADDING - chrome).max(0.0);
    let shown = body.min(room);
    Frame {
        height: chrome + shown,
        body: shown,
        scrolls: body > room,
    }
}

// Centred in the window, never above its top edge.
pub fn origin(window: (f32, f32), card: (f32, f32)) -> (f32, f32) {
    (
        ((window.0 - card.0) / 2.0).max(0.0).round(),
        ((window.1 - card.1) / 2.0).max(0.0).round(),
    )
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ButtonState {
    pub primary: bool,
    pub focused: bool,
    pub destructive: bool,
    pub disabled: bool,
    pub hovered: bool,
    pub pressed: bool,
    // The theme's accent frame is its foreground, so a focused default's hairline would match its rest.
    pub rest_is_foreground: bool,
}

// Which role draws a button's frame, its label and its wash, decided before any colour is chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ink {
    MutedFrame,
    Accent,
    AccentFrame,
    Foreground,
    // A bare error ink draws only a frame; error text and marks on a card take the on-surface role.
    Error,
    ErrorOnSurface,
    None,
}

// A focused control whose foreground hairline would match its rest draws its own frame this wide, inward.
pub const RING: f32 = 2.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ButtonLook {
    pub frame: Ink,
    pub frame_width: f32,
    pub label: Ink,
    // The resting wash under the frame: the default's accent role at 14%, nothing otherwise.
    pub wash: Ink,
    pub wash_alpha: f32,
    // Hover and press lay the label's own ink inside the frame, over the resting wash.
    pub overlay: Ink,
    pub overlay_alpha: f32,
    pub alpha: f32,
    pub pressed: bool,
}

// The default keeps its accent and wash under the ring; a disabled default loses both and takes no pointer wash.
pub fn look(s: ButtonState) -> ButtonLook {
    let label = if s.destructive {
        Ink::ErrorOnSurface
    } else {
        Ink::Foreground
    };
    if s.disabled {
        return ButtonLook {
            frame: Ink::MutedFrame,
            frame_width: BORDER,
            label,
            wash: Ink::None,
            wash_alpha: 0.0,
            overlay: Ink::None,
            overlay_alpha: 0.0,
            alpha: DISABLED_ALPHA,
            pressed: false,
        };
    }
    let frame = if s.focused {
        Ink::Foreground
    } else if s.primary {
        Ink::AccentFrame
    } else {
        Ink::MutedFrame
    };
    let frame_width = if s.focused && s.primary && s.rest_is_foreground {
        RING
    } else {
        BORDER
    };
    let (wash, wash_alpha) = if s.primary {
        (Ink::Accent, PRIMARY_WASH)
    } else {
        (Ink::None, 0.0)
    };
    let (overlay, overlay_alpha) = if s.pressed {
        (label, PRESS_WASH)
    } else if s.hovered {
        (label, HOVER_WASH)
    } else {
        (Ink::None, 0.0)
    };
    ButtonLook {
        frame,
        frame_width,
        label,
        wash,
        wash_alpha,
        overlay,
        overlay_alpha,
        alpha: 1.0,
        pressed: s.pressed,
    }
}

// A press insets only the frame and wash, by whole pixels on each side, so the label never scales.
pub fn press_inset(side: f32, progress: f32) -> f32 {
    (side * (1.0 - PRESS_SCALE) / 2.0 * out_cubic(progress)).round()
}

// Omarchy's OutCubic, the one curve every press and reveal takes.
pub fn out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0) - 1.0;
    t * t * t + 1.0
}

// The scrim takes every pointer input over the window; only a press outside the card dismisses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pointer {
    Hover,
    Wheel,
    Press,
    SecondaryPress,
    Release,
}

pub fn scrim_dismisses(input: Pointer, inside_card: bool) -> bool {
    !inside_card && matches!(input, Pointer::Press | Pointer::SecondaryPress)
}

// A card over Settings dims the window once: the second scrim is never drawn.
pub fn scrim_alpha(cards_below: usize) -> f32 {
    if cards_below == 0 {
        SCRIM_ALPHA
    } else {
        0.0
    }
}

// Jobs card placement and block rhythm.
pub const JOBS_RIGHT: f32 = 14.0;
// The strip sits one status padding before the disk facts.
pub const STRIP_GAP: f32 = 14.0;
pub const JOBS_BOTTOM: f32 = 36.0;
pub const JOBS_WIDTH: f32 = 394.0;
pub const JOBS_PADDING: f32 = 14.0;
pub const JOBS_GAP: f32 = 9.0;
pub const JOBS_MARK: f32 = 16.0;
pub const JOBS_HEAD: f32 = 14.0;
pub const JOBS_HEAD_LINE: f32 = 17.0;
pub const JOBS_SMALL: f32 = 12.0;
pub const JOBS_SMALL_LINE: f32 = 15.0;
pub const JOBS_BAR: f32 = 6.0;
pub const JOBS_TRACK_ALPHA: f32 = 0.25;
pub const JOBS_RULE_MARGIN: f32 = 3.0;
pub const JOBS_WAITING_ALPHA: f32 = 0.55;

pub fn jobs_origin(window: (f32, f32), card_height: f32) -> (f32, f32) {
    let width = JOBS_WIDTH.min((window.0 - 2.0 * JOBS_RIGHT).max(0.0));
    (
        (window.0 - JOBS_RIGHT - width).max(0.0),
        (window.1 - JOBS_BOTTOM - card_height).max(0.0),
    )
}

// The bar's track inside the card's padding and frame.
pub fn jobs_track(window: f32) -> f32 {
    JOBS_WIDTH.min((window - 2.0 * JOBS_RIGHT).max(0.0)) - 2.0 * (JOBS_PADDING + BORDER)
}

// A bar fill never draws past its track; the boards draw the exact share, not a rounded one.
pub fn fill_width(track: f32, share: f64) -> f32 {
    let share = if share.is_nan() {
        0.0
    } else {
        share.clamp(0.0, 1.0)
    };
    track * share as f32
}
