//! The blocks a Markdown document parses to: every field the renderer reads, as plain data the TUI can share.

/// A heading's, a paragraph's or an image row's alignment when it is not the start of the line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Center,
    Right,
}

/// A table column's alignment from its delimiter row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellAlign {
    Left,
    Center,
    Right,
}

/// What a figure draws: a formula or a diagram.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FigureKind {
    Math,
    Mermaid,
}

impl FigureKind {
    /// The kind's name on the helper's wire and in its cache key.
    pub fn name(self) -> &'static str {
        match self {
            FigureKind::Math => "math",
            FigureKind::Mermaid => "mermaid",
        }
    }
}

/// A formula or diagram: its source and whether it is set as a display block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Figure {
    pub kind: FigureKind,
    pub source: String,
    pub display: bool,
}

/// A local picture: its file URL, alt text, sizes from attributes, alignment and the link it opens.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Image {
    pub url: String,
    pub alt: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub align: Option<Align>,
    pub link: Option<String>,
}

/// A quote's text, or its blocks when it holds more than prose; depth past the first level, joined after the first block.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Quote {
    pub text: String,
    pub parts: Option<Vec<Block>>,
    pub depth: Option<usize>,
    pub joined: bool,
}

/// A marker run a list chunk carries: its kind and the widest marker over the whole list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunMarker {
    pub ordered: bool,
    pub marker: String,
}

/// A list as flat entries: each item's text, and the depths, markers, gaps and parts when nesting or looseness needs them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct List {
    pub ordered: bool,
    pub start: u64,
    pub items: Vec<String>,
    pub depths: Option<Vec<usize>>,
    pub markers: Option<Vec<String>>,
    pub gaps: Option<Vec<bool>>,
    pub parts: Option<Vec<Option<Vec<Block>>>>,
    pub last: Option<u64>,
    pub joined: bool,
    pub carry: Option<Vec<RunMarker>>,
    pub tail: Option<Vec<RunMarker>>,
}

/// A table or one chunk of it, with the measures every chunk of the table shares.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Table {
    pub head: Vec<String>,
    pub aligns: Vec<CellAlign>,
    pub rows: Vec<Vec<String>>,
    pub cols: usize,
    pub measure: Vec<String>,
    pub pictures: Vec<String>,
    pub weights: Vec<usize>,
    pub words: Vec<usize>,
    pub joined: bool,
    pub key: Option<usize>,
    pub last: Option<bool>,
}

/// One block of a document.
#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    /// Prose markup, with the inline formulas it holds listed on its first block.
    Run {
        text: String,
        maths: Vec<String>,
    },
    Heading {
        level: u8,
        text: String,
        align: Option<Align>,
    },
    Fence {
        text: String,
        info: String,
    },
    Figure(Figure),
    Quote(Quote),
    List(List),
    Table(Table),
    Image(Image),
    /// A refused remote picture, drawn as its placeholder box.
    Remote {
        host: String,
        align: Option<Align>,
    },
    Images {
        items: Vec<Image>,
        align: Option<Align>,
    },
    /// The sentinel a document nested past the limit parses to.
    Deep {
        limit: usize,
    },
}

impl Block {
    /// The type name the parser's sample inputs are written with.
    pub fn kind(&self) -> &'static str {
        match self {
            Block::Run { .. } => "run",
            Block::Heading { .. } => "heading",
            Block::Fence { .. } => "fence",
            Block::Figure(_) => "figure",
            Block::Quote(_) => "quote",
            Block::List(_) => "list",
            Block::Table(_) => "table",
            Block::Image(_) => "image",
            Block::Remote { .. } => "remote",
            Block::Images { .. } => "images",
            Block::Deep { .. } => "deep",
        }
    }

    /// The block's own markup text, "" for a block that holds none.
    pub fn text(&self) -> &str {
        match self {
            Block::Run { text, .. } | Block::Heading { text, .. } | Block::Fence { text, .. } => {
                text
            }
            Block::Quote(quote) => &quote.text,
            _ => "",
        }
    }

    /// A run of plain markup with no formulas.
    pub fn run(text: impl Into<String>) -> Self {
        Block::Run {
            text: text.into(),
            maths: Vec::new(),
        }
    }
}
