//! The drawn paragraph's parts: a piece's style, an inline picture, the pieces themselves and a paragraph.

use super::Align;

/// How a piece of text draws: weight, slant, lines, size and the chip, link and ink it carries.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub sub: bool,
    pub sup: bool,
    pub small: bool,
    pub mark: bool,
    pub kbd: bool,
    pub code: bool,
    /// A code chip's ground as the parser wrote it, a "#rrggbb" the host turned from its own theme.
    pub chip: Option<String>,
    pub link: Option<String>,
    pub colour: Option<String>,
    pub(super) face: bool,
    pub(super) pad: bool,
}

/// A picture inside a line of text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlineImage {
    pub url: String,
    pub alt: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub link: Option<String>,
}

/// One drawn piece of a paragraph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Piece {
    Text(String, Style),
    Image(InlineImage),
    /// The run's inline formula at this index, drawn as its chip text until its picture lands.
    Formula {
        index: usize,
        text: String,
        style: Style,
    },
    /// A task item's box, ticked or not.
    TaskBox(bool),
    /// A hard line break.
    Break,
    /// The space between two cells of an HTML table row.
    CellGap,
}

/// A paragraph of prose, or a rule across the block.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Para {
    pub rule: bool,
    pub align: Option<Align>,
    /// 1 to 6 for an HTML heading, 0 for prose.
    pub heading: u8,
    /// Preformatted: spaces and line ends kept.
    pub pre: bool,
    /// How many list or quote levels the paragraph sits in.
    pub indent: u8,
    /// An HTML list item's marker, "" for none.
    pub marker: String,
    pub pieces: Vec<Piece>,
}

impl Para {
    /// The paragraph's text alone, pictures and boxes left out.
    pub fn plain(&self) -> String {
        self.pieces
            .iter()
            .map(|piece| match piece {
                Piece::Text(text, _) | Piece::Formula { text, .. } => text.as_str(),
                Piece::Break => "\n",
                Piece::CellGap => "\t",
                _ => "",
            })
            .collect()
    }
}
