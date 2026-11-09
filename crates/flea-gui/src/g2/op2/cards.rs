//! The seam op1's dialog shell and Jobs card fill: an archive card as plain data, a job block for a run.
use crate::list::{Palette, Typeface};
use gpui::{AnyElement, Hsla};

/// Every size below is the board's at its 14 px body; the shell multiplies by the live body over 14 (#310).
pub const BOARD_BODY: f32 = 14.;
/// PR 310's rhythm: 16 padding, 12 between parts, 10 more over the buttons for 22 of air, a 16 title.
pub const PAD: f32 = 16.;
pub const GAP: f32 = 12.;
pub const ABOVE_BUTTONS: f32 = 10.;
pub const TITLE: f32 = 16.;
pub const CAPTION: f32 = 12.;
pub const BUTTON_HEIGHT: f32 = 30.;
pub const BUTTON_PAD: f32 = 9.;
pub const ROW_GAP: f32 = 9.;
/// The Format chips' height, body text and gap, the network card's protocols.
pub const CHIP_HEIGHT: f32 = 30.;
pub const CHIP_GAP: f32 = 4.;
/// The password eye beside the bullets.
pub const EYE: f32 = 13.;
pub const CHECK: f32 = 18.;
/// The tick inside a checked box, the board's 12 px check mark.
pub const CHECK_GLYPH: f32 = 12.;
/// A tracked-capitals label's line and the air under it.
pub const LABEL_LINE: f32 = 14.;
pub const LABEL_GAP: f32 = 5.;
/// The caret's height in a field and the gap between a field's text and its eye.
pub const CARET: f32 = 16.;
pub const FIELD_GAP: f32 = 8.;
/// The FLASH card's image and drive row height, its mark, the text indent past the mark, and a step row.
pub const ROW_HEIGHT: f32 = 31.;
pub const MARK: f32 = 19.;
pub const MARK_INDENT: f32 = 42.;
pub const STEP_ROW: f32 = 37.;
/// The wash under a FLASH row's cursor or selection, and the running bar's empty track.
pub const ROW_WASH: f32 = 0.18;
pub const TRACK_WASH: f32 = 0.25;
/// The hairline rule, the muted frame at 40%.
pub const RULE_WASH: f32 = 0.40;
/// A FLASH row's cursor edge, unscaled like the columns' edge (v0.3.12 hairline * 2).
pub const CURSOR_EDGE: f32 = 2.;
/// The running bar's gap under the step and its height.
pub const BAR_GAP: f32 = 6.;
pub const BAR_HEIGHT: f32 = 6.;

/// The board's marks, drawn by the one shared painter in op3's shell.
pub use crate::g2::op3::shell::Glyph;

/// The board roles a line takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ink {
    Foreground,
    Muted,
    MutedFrame,
    Error,
}

#[derive(Clone, PartialEq)]
pub struct Field {
    pub(crate) input: Option<gpui::Entity<crate::state::Shell>>,
    /// The name above the field in tracked capitals, when the board names it.
    pub label: Option<String>,
    pub value: String,
    pub placeholder: String,
    /// A secret draws bullets, never its characters.
    pub secret: bool,
    /// Bullets standing for a refused answer; a count, never a character of it.
    pub held: usize,
    pub revealed: bool,
    pub eye: bool,
    pub error: bool,
    pub focused: bool,
}

impl Field {
    /// What the field draws: the value while the eye is held, else bullets for a secret.
    pub fn drawn(&self) -> String {
        if self.secret && self.revealed && !self.value.is_empty() {
            return self.value.clone();
        }
        if self.secret && !self.value.is_empty() {
            return "•".repeat(self.value.chars().count());
        }
        if self.secret && self.value.is_empty() && self.held > 0 {
            return "•".repeat(self.held);
        }
        if self.secret {
            return String::new();
        }
        self.value.clone()
    }
}

/// A secret value prints as its length, so a log or a failed assert never carries the password.
impl std::fmt::Debug for Field {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut out = f.debug_struct("Field");
        out.field("label", &self.label);
        if self.secret {
            out.field(
                "value",
                &format_args!("<{} chars>", self.value.chars().count()),
            );
        } else {
            out.field("value", &self.value);
        }
        out.field("placeholder", &self.placeholder)
            .field("secret", &self.secret)
            .field("held", &self.held)
            .field("revealed", &self.revealed)
            .field("eye", &self.eye)
            .field("error", &self.error)
            .field("focused", &self.focused)
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Chips {
    pub options: Vec<String>,
    pub current: usize,
    pub focused: bool,
    /// The tracked-capitals caption over the chips, empty where the board names none.
    pub label: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CheckLine {
    pub checked: bool,
    pub focused: bool,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Chips(Chips),
    Field(Field),
    CheckLine(CheckLine),
    /// A 12 px note at line height 1.5; the wrong-password sentence under the pair.
    Note {
        text: String,
        ink: Ink,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Button {
    pub label: String,
    pub primary: bool,
    pub focused: bool,
    /// A disabled stop leaves the card's own Tab walk.
    pub enabled: bool,
    /// An unavailable action shows muted ink; its keyboard press still reaches the card for its sentence.
    pub available: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Card {
    pub language: String,
    pub title: String,
    pub esc: String,
    /// The board width at the board's body size; the shell scales it.
    pub width: f32,
    pub body: Vec<Block>,
    pub buttons: Vec<Button>,
    pub(super) body_layout: BodyLayout,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BodyLayout {
    Parts,
    Compress,
}

/// Where a click on the card landed: a button, a Format chip, a field by its place among the fields, the checkbox.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Button(usize),
    Chip(usize),
    Field(usize),
    Check,
}

impl Card {
    pub fn focused_button(&self) -> Option<&Button> {
        self.buttons.iter().find(|b| b.focused)
    }
}

/// One run op1's Jobs card draws: the spiral and the head, the file line, the bar, the byte line, Cancel.
#[derive(Clone, Debug, PartialEq)]
pub struct JobBlock {
    pub id: u64,
    pub head: String,
    pub file: String,
    pub byte: Vec<(String, bool)>,
    pub share: f64,
    pub cancel: bool,
}

/// op1's shell owns the scrim, placement and focus trap; a card only hands it plain data.
pub trait DialogShell {
    fn present(
        &self,
        card: &Card,
        palette: &Palette,
        typeface: &Typeface,
        room: f32,
        radius: f32,
    ) -> AnyElement;
}

/// op1's Jobs card owns the transfer card; a run only hands it one block, and the status line its sentences.
pub trait JobsSink {
    fn job(&mut self, block: JobBlock);
    fn close_job(&mut self, id: u64);
    fn say(&mut self, text: String, error: bool);
}

/// The board tokens a card draws with, read from the palette roles the theme fills.
#[derive(Clone, Copy)]
pub(super) struct Tokens {
    /// The card ground; the palette carries no surface role yet, so it is the window ground.
    pub(super) surface: Hsla,
    pub(super) ground: Hsla,
    pub(super) fg: Hsla,
    pub(super) muted: Hsla,
    pub(super) muted_frame: Hsla,
    pub(super) accent: Hsla,
    pub(super) accent_frame: Hsla,
    /// The theme's error role, as an erase warning or a refused field draws it.
    pub(super) error: Hsla,
    pub(super) rule: Hsla,
}

impl Tokens {
    pub(super) fn of(palette: &Palette) -> Self {
        Self {
            surface: palette.background,
            ground: palette.background,
            fg: palette.foreground,
            muted: palette.muted,
            muted_frame: crate::presentation::frames::Frames::from_palette(palette, 0.).ink,
            accent: palette.accent,
            accent_frame: palette.accent_frame,
            error: palette.error,
            rule: crate::presentation::frames::Frames::from_palette(palette, 0.)
                .ink
                .opacity(RULE_WASH),
        }
    }

    pub(super) fn ink(&self, ink: Ink) -> Hsla {
        match ink {
            Ink::Foreground => self.fg,
            Ink::Muted => self.muted,
            Ink::MutedFrame => self.muted_frame,
            Ink::Error => self.error,
        }
    }
}

pub(super) struct Draw<'a> {
    pub(super) language: &'a str,
    pub(super) t: Tokens,
    pub(super) s: f32,
    pub(super) typeface: &'a Typeface,
    /// The live compositor corner value in board units, square at Omarchy's default 0.
    pub(super) r: f32,
    /// The open card's pointer, absent where nothing answers a click.
    pub(super) press: Option<super::draw::CardPress>,
}

pub use super::draw::StandIn;
