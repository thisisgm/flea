//! The Markdown preview's rules: the block model a document parses to, its sentences, line counts and Source chunks.
//! Remote images become a placeholder, only files inside the document's folder load, and links open only by scheme.

#[path = "markdown_blocks.rs"]
pub mod markdown_blocks;
#[path = "markdown_break.rs"]
pub mod markdown_break;
#[path = "markdown_chunks.rs"]
pub mod markdown_chunks;
#[path = "markdown_code_rows.rs"]
pub mod markdown_code_rows;
#[path = "markdown_container.rs"]
pub mod markdown_container;
#[path = "markdown_document.rs"]
pub mod markdown_document;
#[path = "markdown_emph.rs"]
pub mod markdown_emph;
#[path = "markdown_entity.rs"]
pub mod markdown_entity;
#[path = "markdown_entity_table.rs"]
mod markdown_entity_table;
#[path = "markdown_escape.rs"]
pub mod markdown_escape;
#[path = "markdown_event.rs"]
pub mod markdown_event;
#[path = "markdown_figure.rs"]
pub mod markdown_figure;
#[path = "markdown_figure_service.rs"]
pub mod markdown_figure_service;
#[path = "markdown_figure_store.rs"]
pub mod markdown_figure_store;
#[path = "markdown_fit.rs"]
pub mod markdown_fit;
#[path = "markdown_flow.rs"]
pub mod markdown_flow;
#[path = "markdown_html.rs"]
pub mod markdown_html;
#[path = "markdown_html_block.rs"]
pub mod markdown_html_block;
#[path = "markdown_html_heading.rs"]
pub mod markdown_html_heading;
#[path = "markdown_html_image.rs"]
pub mod markdown_html_image;
#[path = "markdown_inline.rs"]
pub mod markdown_inline;
#[path = "markdown_items.rs"]
pub mod markdown_items;
#[path = "markdown_keys.rs"]
pub mod markdown_keys;
#[path = "markdown_layout.rs"]
pub mod markdown_layout;
#[path = "markdown_leaf.rs"]
pub mod markdown_leaf;
#[path = "markdown_link.rs"]
pub mod markdown_link;
#[path = "markdown_lists.rs"]
pub mod markdown_lists;
#[path = "markdown_math.rs"]
pub mod markdown_math;
#[path = "markdown_model.rs"]
pub mod markdown_model;
#[path = "markdown_paragraphs.rs"]
pub mod markdown_paragraphs;
#[path = "markdown_piece.rs"]
pub mod markdown_piece;
#[path = "markdown_prepared.rs"]
pub mod markdown_prepared;
#[path = "markdown_refs.rs"]
pub mod markdown_refs;
#[path = "markdown_resolve.rs"]
pub mod markdown_resolve;
#[path = "markdown_rich.rs"]
pub mod markdown_rich;
#[path = "markdown_rich_text.rs"]
pub mod markdown_rich_text;
#[path = "markdown_run.rs"]
pub mod markdown_run;
#[path = "markdown_source.rs"]
pub mod markdown_source;
#[path = "markdown_text.rs"]
pub mod markdown_text;
#[path = "markdown_url.rs"]
pub mod markdown_url;

pub use markdown_model::{
    Align, Block, CellAlign, Figure, FigureKind, Image, List, Quote, RunMarker, Table,
};

/// A document nesting containers deeper than this is not rendered: it parses to one sentinel and the pane shows its source.
pub const NESTING_LIMIT: usize = 32;
/// A big document's first screen draws from this many blocks, which the parse hands over ahead of the rest.
pub const HEAD_BLOCKS: usize = 96;
/// The deep verdict reads this many bytes of the head at most.
pub const DEEP_HEAD_BYTES: usize = 16384;

/// The sentences the parser writes into the text it draws, each with "%1" for its value so each translates whole.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Words {
    /// A remote image's placeholder, "%1" its host.
    pub remote: String,
}

impl Words {
    /// The English catalog's sentences, for tests and the core's own callers.
    pub fn english() -> Self {
        Self {
            remote: "Remote image not loaded \u{b7} %1".to_string(),
        }
    }
}

/// What a parse is given beside the text: the document's folder, the chip and link colours and the sentences.
#[derive(Clone, Copy, Debug)]
pub struct Ctx<'a> {
    pub dir: &'a str,
    pub chrome: &'a str,
    pub ink: &'a str,
    pub words: &'a Words,
}

impl Ctx<'_> {
    /// A remote image's placeholder for a host already escaped.
    pub fn placeholder(&self, host: &str) -> String {
        markdown_text::fill(&self.words.remote, &[host])
    }
}

/// The blocks a document draws as, in order.
pub fn blocks(source: &str, ctx: &Ctx) -> Vec<Block> {
    markdown_blocks::blocks(source, ctx, 0, &mut |_| {})
}

/// The blocks, with the first `head` of them handed to on_head as soon as they are written.
pub fn blocks_headed(
    source: &str,
    ctx: &Ctx,
    head: usize,
    on_head: &mut dyn FnMut(&[Block]),
) -> Vec<Block> {
    markdown_blocks::blocks(source, ctx, head, on_head)
}

/// The one block a document nested too deep parses to.
pub fn deep_blocks() -> Vec<Block> {
    vec![Block::Deep {
        limit: NESTING_LIMIT,
    }]
}

/// Whether the head of a text nests too deep, and how many bytes the verdict read.
pub fn deep_head(source: &str) -> (bool, usize) {
    let mut head = source;
    if head.len() > DEEP_HEAD_BYTES {
        let cut = markdown_text::rfind_byte(head, b'\n', DEEP_HEAD_BYTES).unwrap_or(0);
        head = &head[..cut];
    }
    (markdown_blocks::nests_too_deep(head), head.len())
}

/// The line count the facts and the bar show: a trailing newline ends a line, only an unterminated tail adds one.
pub fn line_count(text: &str) -> usize {
    markdown_source::line_count(text)
}

/// Every figure block in document order, top level and held by a quote or a list item at any depth.
pub fn figures_in(blocks: &[Block]) -> Vec<&Figure> {
    let mut out = Vec::new();
    collect_figures(blocks, &mut out);
    out
}

fn collect_figures<'a>(blocks: &'a [Block], out: &mut Vec<&'a Figure>) {
    for block in blocks {
        match block {
            Block::Figure(figure) => out.push(figure),
            Block::Quote(quote) => {
                if let Some(parts) = &quote.parts {
                    collect_figures(parts, out);
                }
            }
            Block::List(list) => {
                for parts in list.parts.iter().flatten().flatten() {
                    collect_figures(parts, out);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
#[path = "markdown_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "markdown_oracle_tests.rs"]
mod oracle_tests;

#[cfg(test)]
#[path = "markdown_calls_tests.rs"]
mod calls_tests;

#[cfg(test)]
#[path = "markdown_layout_board_tests.rs"]
mod layout_board_tests;

#[cfg(test)]
#[path = "markdown_linear_tests.rs"]
mod linear_tests;

#[cfg(test)]
#[path = "markdown_layout_column_tests.rs"]
mod layout_column_tests;
