//! The seam op1's dialog shell fills: a card as plain data, and a stand-in frame drawing the boards' anatomy until the shell and scrim land.
use crate::list::{Palette, Typeface};
use gpui::{AnyElement, Div, Hsla, Stateful};

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
/// Open With's application row, its eyebrows and the gaps the board sets over each eyebrow.
pub const APP_ROW: f32 = 37.;
pub const EYEBROW: f32 = 22.;
pub const FIRST_EYEBROW_AIR: f32 = 11.;
pub const LATER_EYEBROW_AIR: f32 = 5.;
pub const ROW_INSET: f32 = 14.;
pub const MARK: f32 = 18.;
pub const CHECK: f32 = 18.;
/// Permissions' subject, grid and owner lines, its grid head and label column.
pub const LINE_40: f32 = 40.;
pub const GRID_HEAD: f32 = 26.;
pub const LABEL_COLUMN: f32 = 96.;
/// The line box of each grid head caption inside the GRID_HEAD row: 1.2 times the caption size.
pub const GRID_HEAD_LINE: f32 = CAPTION * 1.2;
/// Properties' fact lines and the air between its two columns.
pub const FACT_LINE: f32 = 24.;
pub const FACT_GAP: f32 = 18.;
/// Bulk rename's name lines and the error line held under them.
pub const NAME_LINE: f32 = 31.;
pub const ERROR_LINE: f32 = 16.;

/// A card's hairline is the board's rule: the muted frame at 40 percent.
const RULE: f32 = 0.40;
const WASH_DEFAULT: f32 = 0.14;
const WASH_CURSOR: f32 = 0.18;
const HOVER: f32 = 0.08;
const DISABLED: f32 = 0.55;

/// A card's width in the window: the board width grown with the body size, held inside the room less the shell's gap.
pub fn scaled(board: f32, body: f32, room: f32) -> f32 {
    (board * body / BOARD_BODY)
        .ceil()
        .min((room - 2. * GAP).floor())
        .max(0.)
}

/// The board's marks, drawn from their own 24 unit paths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyph {
    Search,
    File,
    Code,
    AppWindow,
    Check,
    Eye,
    Alert,
    Drive,
    Cross,
    FolderOpen,
}

/// The board roles a line takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ink {
    Foreground,
    Muted,
    MutedFrame,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tri {
    On,
    Off,
    Mixed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Check {
    pub value: Tri,
    pub focused: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    /// The name above the field in tracked capitals, when the board names it.
    pub label: Option<String>,
    pub value: String,
    pub placeholder: String,
    pub glyph: Option<Glyph>,
    pub focused: bool,
}

/// Where Open With's cursor row stands: grey while the search holds the keys, the accent wash and edge while the list does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cursor {
    None,
    Waiting,
    Focused,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AppRow {
    Eyebrow {
        text: String,
        first: bool,
    },
    /// `mark` is the window mark the board draws for an entry with no themed icon; a themed icon is the icon service's.
    App {
        label: String,
        tail: String,
        cursor: Cursor,
        mark: Option<Glyph>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct NameLine {
    pub text: String,
    pub ink: Ink,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Fact {
    Text {
        name: String,
        value: String,
        tail: String,
    },
    Tags {
        name: String,
        tags: Vec<String>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Field(Field),
    /// Two fields side by side, 12 apart, as bulk rename's Find and Replace.
    Fields(Field, Field),
    /// Old names beside new names; the new column wears the field frame and holds its error line under it, empty or not, so the card keeps one height.
    Names {
        old_label: String,
        old: Vec<String>,
        new_label: String,
        new: Vec<NameLine>,
        focused: bool,
        error: String,
    },
    /// Open With's list, full bleed, closed by a hairline; the filler holds seven rows' height under a short list.
    Apps(Vec<AppRow>, f32),
    CheckLine {
        check: Check,
        label: String,
    },
    /// A 12 px note at line height 1.5, inset to sit under a checkbox's label when asked.
    Note {
        text: String,
        inset: f32,
        ink: Ink,
        lift: f32,
    },
    /// Permissions' subject line: the mark, the name and the kind word.
    Subject {
        glyph: Glyph,
        name: String,
        kind: String,
    },
    /// The run card's line: the mark in foreground and the kind and size.
    Program {
        glyph: Glyph,
        text: String,
    },
    Rule {
        above: f32,
        below: f32,
    },
    Grid {
        columns: Vec<String>,
        rows: Vec<(String, [Check; 3])>,
    },
    Info {
        label: String,
        value: String,
        tail: String,
    },
    Section {
        head: String,
        lines: Vec<String>,
    },
    Facts(Vec<Fact>),
    /// Parts that sit flush, with none of the card's 12 between them.
    Stack(Vec<Block>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Button {
    pub label: String,
    pub primary: bool,
    pub focused: bool,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Card {
    /// The catalog language controls heading capitals and tracking in the shared shell.
    pub language: String,
    pub title: String,
    pub esc: String,
    /// The board width at the board's body size; the shell scales it.
    pub width: f32,
    /// The rule under the title runs edge to edge, as Open With's board draws it; the other boards keep it inside the padding.
    pub bleed: bool,
    pub body: Vec<Block>,
    pub buttons: Vec<Button>,
}

impl Card {
    pub fn focused_button(&self) -> Option<&Button> {
        self.buttons.iter().find(|b| b.focused)
    }
}

/// op1's shell owns the scrim, placement and focus trap; a card only hands it plain data.
pub trait DialogShell {
    fn present(
        &self,
        card: &Card,
        palette: &Palette,
        typeface: &Typeface,
        corner: f32,
        room: f32,
    ) -> AnyElement;
}

/// The board tokens a card draws with, read from the palette roles the theme fills.
#[derive(Clone, Copy)]
struct Tokens {
    /// The card ground; the palette carries no surface role yet, so it is the window ground.
    surface: Hsla,
    ground: Hsla,
    fg: Hsla,
    muted: Hsla,
    muted_frame: Hsla,
    accent: Hsla,
    accent_frame: Hsla,
    /// The palette carries no error role yet, so an error wears the accent until it does.
    error: Hsla,
    rule: Hsla,
}

impl Tokens {
    fn of(palette: &Palette) -> Self {
        Self {
            surface: palette.background,
            ground: palette.background,
            fg: palette.foreground,
            muted: palette.metadata,
            muted_frame: palette.metadata_frame,
            accent: palette.accent,
            accent_frame: palette.accent_frame,
            error: palette.accent,
            rule: palette.metadata_frame.opacity(RULE),
        }
    }

    fn ink(&self, ink: Ink) -> Hsla {
        match ink {
            Ink::Foreground => self.fg,
            Ink::Muted => self.muted,
            Ink::MutedFrame => self.muted_frame,
            Ink::Error => self.error,
        }
    }
}

struct Draw<'a> {
    corner: f32,
    t: Tokens,
    s: f32,
    typeface: &'a Typeface,
    language: &'a str,
    wire: Option<&'a Wire>,
    entry: Option<&'a crate::path::entry::Entry>,
}

/// The controls whose pointer activation a live card serves through its model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    Field,
    Check(usize),
    Button(usize),
}

pub type Wire = dyn Fn(Control, Stateful<Div>) -> Stateful<Div>;

/// The same board drawing with live controls and the window's card surface role.
pub struct Interactive<'a> {
    pub wire: &'a Wire,
    pub surface: Hsla,
    pub entry: &'a crate::path::entry::Entry,
}

mod blocks;
#[cfg(test)]
mod caption_tests;
mod draw;
mod glyph;
pub use draw::StandIn;
pub use glyph::glyph;
