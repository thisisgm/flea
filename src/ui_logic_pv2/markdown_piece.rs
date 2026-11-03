//! The pieces an inline scan writes before it joins them: text, held tokens and emphasis marks, and the line state they carry.

use super::markdown_entity::{drawn_text, reference_at};
use super::markdown_text::at;

/// One piece of the scan's output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Piece {
    Ch(char),
    Lit(&'static str),
    Text(String),
    /// A held token by its index in the token table.
    Token(usize),
    /// An emphasis delimiter run by its index in the delimiter list.
    Delim(usize),
}

impl Piece {
    /// The piece's text when it is text.
    pub fn text(&self) -> Option<std::borrow::Cow<'_, str>> {
        match self {
            Piece::Ch(c) => Some(std::borrow::Cow::Owned(c.to_string())),
            Piece::Lit(s) => Some(std::borrow::Cow::Borrowed(s)),
            Piece::Text(s) => Some(std::borrow::Cow::Borrowed(s)),
            Piece::Token(_) | Piece::Delim(_) => None,
        }
    }

    /// True when the piece is exactly this text.
    pub fn is(&self, text: &str) -> bool {
        match self {
            Piece::Ch(c) => {
                let mut buf = [0u8; 4];
                c.encode_utf8(&mut buf) == text
            }
            Piece::Lit(s) => *s == text,
            Piece::Text(s) => s == text,
            Piece::Token(_) | Piece::Delim(_) => false,
        }
    }
}

/// An emphasis delimiter run: its mark, marks left and at first, its piece index and end, what it can do and what it became.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Delim {
    pub ch: u8,
    pub n: usize,
    pub orig: usize,
    pub at: usize,
    pub end: usize,
    pub open: bool,
    pub close: bool,
    pub before: String,
    pub after: String,
    pub opens: usize,
    pub closes: usize,
}

/// The line break a hard break writes.
pub const HARD_BREAK: &str = "<br />";
// What a line holding only space references keeps, so the renderer reads text and the paragraph stays whole.
const WHOLE_LINE_REFERENCE: &str = "&#32;";

fn blank_piece(text: &str) -> bool {
    text.bytes().all(|b| b == b' ')
}

/// Whether the line is still blank after a piece: a newline restarts it, "<br />" ends it, a token or other text fills it.
pub fn blank_after(blank: bool, piece: &Piece) -> bool {
    let Some(text) = piece.text() else {
        return false;
    };
    if let Some(cut) = text.rfind('\n') {
        return blank_piece(&text[cut + 1..]);
    }
    text == HARD_BREAK || (blank && blank_piece(&text))
}

/// The scan's line state, folded once per piece so it stays linear.
#[derive(Clone, Copy, Debug)]
pub struct LineState {
    pub scanned: usize,
    pub blank: bool,
}

impl LineState {
    /// A fresh line, blank unless the text is a fragment that starts mid-line.
    pub fn new(starts_blank: bool) -> Self {
        Self {
            scanned: 0,
            blank: starts_blank,
        }
    }

    /// Whether the line holds nothing drawn after every piece so far.
    pub fn blank(&mut self, out: &[Piece]) -> bool {
        self.scanned = self.scanned.min(out.len());
        while self.scanned < out.len() {
            self.blank = blank_after(self.blank, &out[self.scanned]);
            self.scanned += 1;
        }
        self.blank
    }

    /// Restarts the state after a label's pieces were replaced by one.
    pub fn restart(&mut self, out_len: usize, piece: &Piece) {
        self.blank = blank_after(false, piece);
        self.scanned = out_len;
    }
}

/// A run of space references at i: what it keeps, the index after it and how many space pieces before it to drop.
pub struct SpaceRun {
    pub text: &'static str,
    pub end: usize,
    pub trim: usize,
}

/// "x  &#32;\ny" at 3 answers ("", 8, 2); "x&#32;  \ny" at 1 answers ("", 6, 0); a line holding only the run keeps one reference.
pub fn space_run_at(
    text: &str,
    i: usize,
    out: &[Piece],
    blank: bool,
    ends_line: bool,
) -> Option<SpaceRun> {
    let mut end = i;
    let mut last_reference = i;
    let mut hit = reference_at(text, end);
    while let Some((decoded, after)) = hit.as_ref() {
        if drawn_text(decoded) != " " {
            break;
        }
        end = *after;
        last_reference = end;
        while at(text, end) == b' ' {
            end += 1;
        }
        hit = reference_at(text, end);
    }
    if last_reference == i {
        return None;
    }
    if (end >= text.len() && ends_line) || at(text, end) == b'\n' {
        let trim = out.iter().rev().take_while(|piece| piece.is(" ")).count();
        return Some(SpaceRun {
            text: if blank { WHOLE_LINE_REFERENCE } else { "" },
            end: last_reference,
            trim,
        });
    }
    Some(SpaceRun {
        text: if blank { "" } else { " " },
        end,
        trim: 0,
    })
}
