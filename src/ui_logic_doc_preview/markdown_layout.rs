//! The blocks of a document laid out at a width: every drawn item's place inside its block, and how blocks stack.

use super::markdown_figure::pair_trim;
use super::markdown_flow::{Base, Face, FormulaBox, Ink, Laid, Measure, Sizes};
use super::markdown_model::{Align, Block, FigureKind};

#[path = "markdown_layout_asks.rs"]
mod asked;
#[path = "markdown_layout_blocks.rs"]
mod blocks;
#[path = "markdown_layout_table.rs"]
mod table;

pub use asked::{asks, count, Asks, NoSizes, RunSizes};
pub use blocks::{fence_base, heading_voice, layout, prose, Voice};

/// A line box is this many times its text's size.
pub const LINE_RATIO: f32 = 1.7;
/// The board's pixels are drawn at this body size, and each follows the theme's body from there.
pub const BOARD_BODY: f32 = 14.0;
/// The space each table column keeps beside its text, and a cell's padding above and below.
pub const CELL_GAP: f32 = 14.0;
pub const CELL_PAD: f32 = 2.0;
/// One glyph's width over the size when a table measured no text.
pub const FALLBACK_GLYPH_EM: f32 = 0.6;
/// A table wider than its block by more than this scrolls sideways.
pub const OVERFLOW_PX: f32 = 0.5;
/// A quote's bar is this wide, the theme's gap right of the one before it.
pub const QUOTE_BAR: f32 = 2.0;

/// The theme's sizes a document lays out with, from the host's scaled tokens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    /// The document's body size: the theme's body in Quick Look, the next size under it in the column.
    pub body: f32,
    /// The theme's body size, which the board's pixels scale with.
    pub theme_body: f32,
    pub caption: f32,
    /// The theme's gap: beside a list marker and a quote bar, and inside the remote box.
    pub gap: f32,
    pub hairline: f32,
    /// The theme's row padding, which a sideways table keeps under its last row for the scroll lane.
    pub row_pad_x: f32,
    /// The chrome mark's size, the remote box's picture glyph.
    pub mark: f32,
    /// True for the preview column's document, which the column board draws at its own line boxes and margins.
    pub column: bool,
}

impl Metrics {
    /// A board pixel at the theme's body size.
    pub fn board(&self, px: f32) -> f32 {
        (px * self.theme_body / BOARD_BODY).round()
    }

    pub fn line(px: f32) -> f32 {
        (px * LINE_RATIO).round()
    }

    pub fn block_gap(&self) -> f32 {
        self.board(6.0)
    }

    /// A fence's padding across, above and below. Quick Look draws 8 px above and below; the column draws 9 and 7.
    pub fn fence_pad(&self) -> (f32, f32, f32) {
        if self.column {
            (self.board(12.0), self.board(9.0), self.board(7.0))
        } else {
            (self.board(12.0), self.board(8.0), self.board(8.0))
        }
    }

    /// A fence's line: 20 board pixels in Quick Look, 18 in the column.
    pub fn fence_line(&self) -> f32 {
        self.board(if self.column { 18.0 } else { 20.0 })
    }

    /// The rendered document's inset down: 18 board pixels in Quick Look, 20 in the column.
    pub fn inset_y(&self) -> f32 {
        self.board(if self.column { 20.0 } else { 18.0 })
    }

    /// Headings are 20 and 15 board pixels; deeper levels take the body.
    pub fn heading_px(&self, level: u8) -> f32 {
        match level {
            1 => self.board(20.0),
            2 => self.board(15.0),
            _ => self.body,
        }
    }

    /// A heading's line box. Quick Look's h1 is 32 board pixels and its h2 takes the body line; the column's h1 is 28.
    /// Other headings keep the oracle's 1.7 times their size.
    pub fn heading_line(&self, level: u8) -> f32 {
        match (self.column, level) {
            (false, 1) => self.board(32.0),
            (false, 2) => Metrics::line(self.body),
            (true, 1) => self.board(28.0),
            _ => Metrics::line(self.heading_px(level)),
        }
    }

    /// The space a block lends above itself. Quick Look's h2 has 8 px and its table 7; every other case,
    /// in both surfaces, takes the oracle's 6 px gap, since no board draws it.
    pub fn above(&self, block: &Block) -> f32 {
        match block {
            Block::Heading { level: 2, .. } if !self.column => self.board(8.0),
            Block::Table(_) if !self.column => self.board(7.0),
            _ => self.block_gap(),
        }
    }

    /// The space a block lends below itself: the column's h1 keeps 8 px under it.
    pub fn below(&self, block: &Block) -> f32 {
        match block {
            Block::Heading { level: 1, .. } if self.column => self.board(8.0),
            _ => 0.0,
        }
    }

    /// A row of single-line table cells with its rule under it: the header and body rows are 28 and 29 board pixels in
    /// Quick Look, and every row is 27 in the column.
    pub fn table_row(&self, head: bool) -> f32 {
        match (self.column, head) {
            (false, true) => self.board(28.0),
            (false, false) => self.board(29.0),
            (true, _) => self.board(27.0),
        }
    }

    /// The paragraph base of a size, face and ink, its line box from the size.
    pub fn base(&self, px: f32, face: Face, ink: Ink, align: Option<Align>) -> Base {
        Base {
            px,
            face,
            ink,
            line: Metrics::line(px),
            align,
            chip_pad: (self.board(if self.column { 3.0 } else { 4.0 }) * px / self.body).round(),
            chip_tall: (self.board(20.0) * px / self.body).round(),
            script_px: (px * 9.0 / BOARD_BODY).round(),
            sub_shift: (px * 2.0 / BOARD_BODY).round(),
            sup_shift: -(px * 8.0 / BOARD_BODY).round(),
            cell_gap: CELL_GAP,
            pre: false,
        }
    }
}

/// A figure's state as the host holds it: still asked, drawn at its natural size, or failed to its source.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FigureState {
    Waiting,
    Drawn { width: f32, height: f32 },
    Failed,
}

/// What the host has for a document's pictures, figures, inline formulas and fence colours, each by its ordinal.
pub trait Assets {
    fn picture(&self, url: &str) -> Option<(f32, f32)>;
    fn figure(&self, ordinal: usize) -> FigureState;
    fn formula(&self, ordinal: usize) -> Option<FormulaBox>;
    /// A fence line's runs as the code reply gives them, the line 1-based; None draws it in the foreground.
    fn code(&self, fence: usize, line: usize) -> Option<Vec<(i64, String)>>;
}

/// How many figures, inline formulas and fences come before a block, in document order at every depth.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counter {
    pub figure: usize,
    pub formula: usize,
    pub fence: usize,
}

/// One drawn thing of a block, its place from the block's top left.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Text {
        x: f32,
        y: f32,
        laid: Laid,
    },
    /// A fence's or a failed figure's ground; a fence names its ordinal so its colours can be asked.
    Ground {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        fence: Option<usize>,
    },
    Bar {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
    /// A table rule in the faint wash, or a thematic break in the frame ink.
    Rule {
        x: f32,
        y: f32,
        width: f32,
        wash: bool,
    },
    Dashed {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
    /// The remote box's picture glyph.
    Glyph {
        x: f32,
        y: f32,
        size: f32,
    },
    Picture {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        url: String,
        link: Option<String>,
        stretch: bool,
    },
    /// A display figure, 0 high while it is asked.
    Figure {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        ordinal: usize,
    },
    /// A table chunk wider than its block: its items scroll sideways together inside `width`.
    Table {
        key: Option<usize>,
        x: f32,
        y: f32,
        width: f32,
        content: f32,
        height: f32,
        items: Vec<Item>,
    },
}

/// A laid block: its height, its items and the first text baseline a list marker lines up with.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Geometry {
    pub height: f32,
    pub items: Vec<Item>,
    pub first_baseline: Option<f32>,
}

impl Geometry {
    /// Moves every item by (dx, dy) and adds them to this block.
    pub fn absorb(&mut self, other: Geometry, dx: f32, dy: f32) {
        for item in other.items {
            self.items.push(shifted(item, dx, dy));
        }
    }
}

/// A remote box's dash and the gap after it.
pub const DASH_PX: f32 = 3.0;

/// The four dashed sides of a box as (x, y, width, height) rects, each side dashing from its own corner.
/// A side of 20 holds three dashes, at 0, 6 and 12, the last ending at 15.
pub fn dashes(width: f32, height: f32, hairline: f32) -> Vec<(f32, f32, f32, f32)> {
    let end = |length: f32| {
        ((((length + DASH_PX) / (2.0 * DASH_PX)).floor().max(1.0)) - 1.0) * 2.0 * DASH_PX + DASH_PX
    };
    let mut out = Vec::new();
    let mut at = 0.0;
    while at < end(width) {
        out.push((at, 0.0, DASH_PX, hairline));
        out.push((at, height - hairline, DASH_PX, hairline));
        at += 2.0 * DASH_PX;
    }
    let mut at = 0.0;
    while at < end(height) {
        out.push((0.0, at, hairline, DASH_PX));
        out.push((width - hairline, at, hairline, DASH_PX));
        at += 2.0 * DASH_PX;
    }
    out
}

/// The item moved by (dx, dy).
pub fn shifted(mut item: Item, dx: f32, dy: f32) -> Item {
    match &mut item {
        Item::Text { x, y, .. }
        | Item::Ground { x, y, .. }
        | Item::Bar { x, y, .. }
        | Item::Rule { x, y, .. }
        | Item::Dashed { x, y, .. }
        | Item::Glyph { x, y, .. }
        | Item::Picture { x, y, .. }
        | Item::Figure { x, y, .. }
        | Item::Table { x, y, .. } => {
            *x += dx;
            *y += dy;
        }
    }
    item
}

/// What a layout is given beside the blocks: the sizes, the text system, the host's assets and the ordinals so far.
pub struct Ctx<'a> {
    pub metrics: Metrics,
    pub measure: &'a mut dyn Measure,
    pub assets: &'a dyn Assets,
    pub counter: Counter,
    /// The remote placeholder's sentence, "%1" for the host.
    pub remote: &'a str,
}

fn is_picture(block: &Block) -> bool {
    matches!(
        block,
        Block::Image(_) | Block::Images { .. } | Block::Remote { .. } | Block::Figure(_)
    )
}

fn joined(block: &Block) -> bool {
    match block {
        Block::Quote(quote) => quote.joined,
        Block::List(list) => list.joined,
        Block::Table(table) => table.joined,
        _ => false,
    }
}

/// What block i adds under itself: a picture's gap when text follows it, less the canvas margins two figures in a row lend back.
pub fn tail(blocks: &[Block], i: usize, gap: f32) -> f32 {
    let next = blocks.get(i + 1);
    let picture_gap = match (&blocks[i], next) {
        (Block::Image(_) | Block::Images { .. }, Some(next)) if !is_picture(next) => gap,
        _ => 0.0,
    };
    let kind_of = |block: Option<&Block>| match block {
        Some(Block::Figure(figure)) => Some(figure.kind),
        _ => None,
    };
    let trim = match kind_of(Some(&blocks[i])) {
        Some(kind) => pair_trim(kind, kind_of(next)) as f32,
        None => 0.0,
    };
    picture_gap - trim
}

/// The space above block i: the larger of its own top margin and the bottom margin of the block above, none above a chunk
/// that joins the one before.
pub fn lead(blocks: &[Block], i: usize, metrics: &Metrics) -> f32 {
    if i == 0 || joined(&blocks[i]) {
        0.0
    } else {
        metrics.above(&blocks[i]).max(metrics.below(&blocks[i - 1]))
    }
}

/// Every block's top from its height, by the margins, joins, picture gaps and figure trims; and the list's height.
pub fn tops(blocks: &[Block], heights: &[f32], metrics: &Metrics) -> (Vec<f32>, f32) {
    let gap = metrics.block_gap();
    let mut tops = Vec::with_capacity(blocks.len());
    let mut y = 0.0;
    for (i, height) in heights[..blocks.len()].iter().enumerate() {
        y += lead(blocks, i, metrics);
        tops.push(y);
        y += height + tail(blocks, i, gap);
    }
    (tops, y.max(0.0))
}

/// The blocks an item or a quote holds, laid in order at the width with the list's gap.
pub fn stack(blocks: &[Block], width: f32, ctx: &mut Ctx) -> Geometry {
    let mut out = Geometry::default();
    let mut heights = Vec::with_capacity(blocks.len());
    let mut laid = Vec::with_capacity(blocks.len());
    for block in blocks {
        let geometry = layout(block, width, ctx);
        heights.push(geometry.height);
        laid.push(geometry);
    }
    let (tops, height) = tops(blocks, &heights, &ctx.metrics);
    // Only a first block of plain prose gives the baseline a list marker lines up with.
    if let (Some(Block::Run { maths, .. }), Some(first)) = (blocks.first(), laid.first()) {
        if maths.is_empty() {
            out.first_baseline = first.first_baseline.map(|b| b + tops[0]);
        }
    }
    for (geometry, top) in laid.into_iter().zip(tops) {
        out.absorb(geometry, 0.0, top);
    }
    out.height = height;
    out
}
