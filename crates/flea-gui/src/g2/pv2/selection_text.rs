//! The text a range copies: original bytes kept beside the tab-filled text the preview draws.
use super::Position;
use flea::ui_logic_pv2::code::TAB;
use std::{ops::Range, sync::Arc};

/// A logical text chunk, with the original tabs and line endings kept for copying.
#[derive(Clone)]
pub struct Chunk {
    pub text: String,
    pub(super) raw: String,
}

impl Chunk {
    pub fn new(raw: String) -> Self {
        let mut column = 0;
        let mut text = String::new();
        for c in raw.chars() {
            if c == '\t' {
                let n = TAB - column % TAB;
                text.extend(std::iter::repeat_n(' ', n));
                column += n;
            } else {
                text.push(c);
                column = if c == '\n' { 0 } else { column + 1 };
            }
        }
        Self { text, raw }
    }

    /// The raw offset of a shown byte; one inside a tab's fill takes the whole tab, rounding down at a start.
    pub(super) fn raw_at(&self, byte: usize, up: bool) -> usize {
        let (mut column, mut shown) = (0, 0);
        for (at, c) in self.raw.char_indices() {
            let width = if c == '\t' {
                TAB - column % TAB
            } else {
                c.len_utf8()
            };
            if shown >= byte || (!up && byte < shown + width) {
                return at;
            }
            shown += width;
            column = match c {
                '\n' => 0,
                '\t' => column + width,
                _ => column + 1,
            };
        }
        self.raw.len()
    }

    /// The raw bytes of chunk `id` that a range covers.
    pub(super) fn raw_span(&self, id: usize, range: &Range<Position>) -> Range<usize> {
        let from = if id == range.start.chunk {
            self.raw_at(range.start.byte, false)
        } else {
            0
        };
        let to = if id == range.end.chunk {
            self.raw_at(range.end.byte, true)
        } else {
            self.raw.len()
        };
        from..to
    }
}

/// A document's original text and its line starts; a line's chunk is cut only when a range reaches it.
#[derive(Clone)]
pub struct Lines {
    text: Arc<str>,
    starts: Arc<Vec<usize>>,
}

impl Lines {
    pub fn new(text: Arc<str>, starts: Arc<Vec<usize>>) -> Self {
        Self { text, starts }
    }

    pub(super) fn raw(&self, line: usize) -> Option<&str> {
        let start = *self.starts.get(line)?;
        let end = self
            .starts
            .get(line + 1)
            .copied()
            .unwrap_or(self.text.len());
        self.text.get(start..end)
    }
}
