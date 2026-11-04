//! What the block reader hands the writer: one event per line, or a heading, a table or a display formula it read whole.

use super::markdown_model::{CellAlign, FigureKind};
use super::markdown_refs::Lines;

/// What a line is to the writer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Run,
    Setext,
    Hidden,
    FenceOpen,
    FenceBody,
    FenceClose,
    CodeLine,
    CodeEnd,
}

impl LineKind {
    /// Lines inside a fence or an indented block, which the inline pass never sees.
    pub fn raw(self) -> bool {
        matches!(
            self,
            LineKind::FenceOpen | LineKind::FenceBody | LineKind::FenceClose | LineKind::CodeLine
        )
    }
}

/// One open container: a quote, or a list item with its content column, marker kind and the list group it joins.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub id: usize,
    pub quote: bool,
    pub content_col: usize,
    pub ordered: bool,
    pub mark: u8,
    pub start: u64,
    pub bare_at: Option<usize>,
    pub group: usize,
    pub sealed: bool,
}

/// The leading frames now open that share frame 0's type, where the line's text starts after them, and how many frames stayed open.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Lead {
    pub n: usize,
    pub here: usize,
    pub lazy: bool,
    pub retained: usize,
    pub at: usize,
    pub pad: usize,
    pub col: usize,
}

/// One line as the reader saw it: its kind, index and container-free text, its frames and the raw line.
pub struct LineEvent<'a> {
    pub kind: LineKind,
    pub index: usize,
    pub text: String,
    pub info: String,
    pub figure: Option<FigureKind>,
    pub chain: &'a [Frame],
    pub lead: &'a Lead,
    pub raw: &'a str,
    pub state: &'a Lines,
}

impl LineEvent<'_> {
    /// The outermost frame the line sits in.
    pub fn top(&self) -> Option<&Frame> {
        self.chain.first()
    }
}

/// One event of the block reader.
pub enum Event<'a> {
    Line(LineEvent<'a>),
    Heading {
        level: u8,
        text: String,
    },
    Table {
        head: Vec<String>,
        aligns: Vec<CellAlign>,
        rows: Vec<Vec<String>>,
    },
    Math {
        source: String,
    },
}

/// A line held by a container or a run: its text, index, whether it is code, and whether the pass hid it or escaped its bracket.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub text: String,
    pub index: usize,
    pub raw: bool,
    pub hidden: bool,
    pub escaped: bool,
}

impl Line {
    /// A line read off an event with the given text, its visibility fixed by the pass's state.
    pub fn of(event: &LineEvent, text: String) -> Self {
        Self {
            text,
            index: event.index,
            raw: event.kind.raw(),
            hidden: event.state.hidden.contains_key(&event.index),
            escaped: event.state.escaped.contains(&event.index),
        }
    }
}
