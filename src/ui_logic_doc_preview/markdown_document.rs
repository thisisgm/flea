//! The writer: the block reader's events serialized into blocks; every container and code decision belongs to the reader.

use super::markdown_blocks::block_pass;
use super::markdown_chunks::chunk_list;
use super::markdown_container::expand_lead;
use super::markdown_entity::decode_references;
use super::markdown_event::{Event, Line, LineEvent, LineKind};
use super::markdown_items::{list_block, quote_blocks, visible_lines, Builder, Host, QuoteLine};
use super::markdown_leaf::{
    chunk_table, delim_aligns, heading_safe, is_thematic, setext_level, table_block,
};
use super::markdown_model::{Block, Figure, FigureKind, List};
use super::markdown_refs::{Lines, Refs};
use super::markdown_run::{parse_inline, Env, Shape};
use super::markdown_text::{blank, trim};
use super::Ctx;

#[path = "markdown_document_run.rs"]
pub(super) mod run;
use run::math_figure;

/// Items and quotes hold their blocks as parts down to this many levels; deeper text stays one markup text, which keeps a deep chain linear.
pub const NEST_LIMIT: usize = 8;

/// What every writer of one document shares: the references, the held-span tokens and the notes cited in order.
pub struct Shared<'r> {
    pub refs: &'r mut Refs,
    pub tokens: Vec<String>,
    pub cited: Vec<String>,
}

struct Outer {
    quote: bool,
    group: usize,
    lines: Vec<QuoteLine>,
    ordered: bool,
    start: u64,
    builder: Builder,
}

struct Code {
    lines: Vec<String>,
    info: String,
    figure: Option<FigureKind>,
    indented: bool,
}

/// One writer: the document's own, or one for an item's or a quote's parts, which numbers notes and tokens with the document's.
pub struct Writer<'a, 'r> {
    ctx: &'a Ctx<'a>,
    shared: &'a mut Shared<'r>,
    level: usize,
    nested: bool,
    /// The blocks written so far, final once written.
    pub out: Vec<Block>,
    run: Vec<String>,
    outer: Option<Outer>,
    code: Option<Code>,
}

// A delimiter row the reader refused as a table keeps its first dash from the renderer, which would make a table of it.
fn tableless(text: &str) -> String {
    if text.contains('|') && delim_aligns(text).is_some() {
        text.replacen('-', "\\-", 1)
    } else {
        text.to_string()
    }
}

// The line's text once the list markers and indents around it are gone; a lazy line has none to remove.
fn lead_text(event: &LineEvent) -> String {
    let lead = event.lead;
    if lead.here == lead.n && !lead.lazy {
        format!(
            "{}{}",
            " ".repeat(lead.pad),
            expand_lead(&event.raw[lead.at..], lead.col + lead.pad)
        )
    } else {
        event.text.clone()
    }
}

impl<'a, 'r> Writer<'a, 'r> {
    /// A writer at a nesting level; a nested one lists no notes of its own.
    pub fn new(ctx: &'a Ctx<'a>, shared: &'a mut Shared<'r>, level: usize, nested: bool) -> Self {
        Self {
            ctx,
            shared,
            level,
            nested,
            out: Vec::new(),
            run: Vec::new(),
            outer: None,
            code: None,
        }
    }

    fn inline_with(&mut self, text: &str, cite: bool, shape: Shape) -> String {
        let refs = &mut *self.shared.refs;
        let mut env = Env {
            ctx: self.ctx,
            defs: &refs.defs,
            numbers: &mut refs.numbers,
            tokens: &mut self.shared.tokens,
            cited: if cite {
                Some(&mut self.shared.cited)
            } else {
                None
            },
        };
        parse_inline(text, &mut env, shape)
    }

    // The underline of a setext heading takes the paragraph above it out of the pending run and draws it as a heading.
    fn setext(&mut self, underline: &str) {
        let mut from = self.run.len();
        while from > 0 && !blank(&self.run[from - 1]) && !is_thematic(&self.run[from - 1]) {
            from -= 1;
        }
        let paragraph = self.run.split_off(from);
        self.flush_run();
        let text = heading_safe(trim(&paragraph.join("\n")));
        if !text.is_empty() {
            let text = self.inline_with(&text, true, Shape::default());
            self.out.push(Block::Heading {
                level: setext_level(underline),
                text,
                align: None,
            });
        }
    }

    fn emit_code(&mut self, code: Code) {
        self.flush_run();
        let source = code.lines.join("\n");
        self.out.push(match code.figure {
            Some(kind) => Block::Figure(Figure {
                kind,
                source,
                display: true,
            }),
            None => Block::Fence {
                text: source,
                info: decode_references(&code.info),
            },
        });
    }

    fn emit_outer(&mut self, outer: Outer) {
        self.flush_run();
        if outer.quote {
            let blocks = quote_blocks(&outer.lines, self);
            self.out.extend(blocks);
        } else {
            let list: List = list_block(&outer.builder, outer.ordered, outer.start, self);
            if !list.items.is_empty() {
                self.out.extend(chunk_list(list));
            }
        }
    }

    fn emit_whole(&mut self, event: Event) {
        self.flush_run();
        match event {
            Event::Heading { level, text } => {
                let text = self.inline_with(&heading_safe(&text), true, Shape::default());
                self.out.push(Block::Heading {
                    level,
                    text,
                    align: None,
                });
            }
            Event::Table { head, aligns, rows } => {
                let shape = Shape {
                    literal_plain: true,
                    ..Shape::default()
                };
                let table = table_block(&head, aligns, &rows, &mut |text| {
                    self.inline_with(text, true, shape)
                });
                self.out.extend(chunk_table(table));
            }
            Event::Math { source } => self.out.push(math_figure(source)),
            Event::Line(_) => {}
        }
    }

    fn flush_outer(&mut self) {
        if let Some(outer) = self.outer.take() {
            self.emit_outer(outer);
        }
    }

    fn flush_code(&mut self) {
        let Some(mut code) = self.code.take() else {
            return;
        };
        if code.indented {
            while code.lines.last().is_some_and(|line| blank(line)) {
                code.lines.pop();
            }
        }
        self.emit_code(code);
    }

    /// Takes one event of the block reader.
    pub fn project(&mut self, event: Event) {
        let Event::Line(line) = event else {
            self.flush_outer();
            self.flush_code();
            return self.emit_whole(event);
        };
        let group = line
            .top()
            .map(|top| if top.quote { top.id } else { top.group });
        if self
            .outer
            .as_ref()
            .is_some_and(|outer| Some(outer.group) != group)
        {
            self.flush_outer();
        }
        if let (Some(top), Some(group)) = (line.top(), group) {
            self.flush_code();
            let outer = self.outer.get_or_insert_with(|| Outer {
                quote: top.quote,
                group,
                lines: Vec::new(),
                ordered: top.ordered,
                start: top.start,
                builder: Builder::default(),
            });
            if line.kind == LineKind::CodeEnd {
                return;
            }
            let text = lead_text(&line);
            if outer.quote {
                outer.lines.push(QuoteLine {
                    line: Line::of(&line, text),
                    depth: line.lead.n,
                });
            } else {
                outer.builder.add(&line, text);
            }
            return;
        }
        match line.kind {
            LineKind::Hidden => {}
            LineKind::FenceOpen => {
                self.flush_code();
                self.code = Some(Code {
                    lines: Vec::new(),
                    info: line.info.clone(),
                    figure: line.figure,
                    indented: false,
                });
            }
            LineKind::FenceBody | LineKind::CodeLine => {
                let code = self.code.get_or_insert_with(|| Code {
                    lines: Vec::new(),
                    info: String::new(),
                    figure: None,
                    indented: true,
                });
                code.lines.push(line.text.clone());
            }
            LineKind::FenceClose | LineKind::CodeEnd => self.flush_code(),
            LineKind::Setext => {
                self.flush_code();
                self.setext(&line.text);
            }
            LineKind::Run => {
                self.flush_code();
                let kept = visible_lines(&[Line::of(&line, tableless(&line.text))]);
                self.run.extend(kept.texts);
            }
        }
    }

    /// The document's blocks once every event is in, the cited notes listed last by the document's own writer.
    pub fn finish(mut self) -> Vec<Block> {
        self.flush_outer();
        // A fence left open at the end of the text holds no line for the final newline.
        if let Some(code) = self.code.as_mut().filter(|code| !code.indented) {
            if code.lines.last().is_some_and(String::is_empty) {
                code.lines.pop();
            }
        }
        self.flush_code();
        self.flush_run();
        if self.nested {
            return self.out;
        }
        let cited = self.shared.cited.clone();
        let mut items = Vec::with_capacity(cited.len());
        for id in &cited {
            let number = self.shared.refs.numbers.get(id).copied().unwrap_or(0);
            let note = self.shared.refs.notes.get(id).cloned().unwrap_or_default();
            let text = self.inline_with(&note, false, Shape::default());
            items.push(format!("<sup>{number}</sup> {text}"));
        }
        if !items.is_empty() {
            self.out.push(Block::run("---"));
            self.out.push(Block::List(List {
                ordered: false,
                start: 0,
                items,
                ..List::default()
            }));
        }
        self.out
    }
}

impl Host for Writer<'_, '_> {
    fn inline(&mut self, text: &str) -> String {
        self.inline_with(text, true, Shape::default())
    }

    // The lines "a", "```", "x", "```" hold a run and a fence; None past NEST_LIMIT, where the lines stay one text.
    fn parts(&mut self, lines: &[String]) -> Option<Vec<Block>> {
        if self.level >= NEST_LIMIT {
            return None;
        }
        let mut own = Lines::default();
        let ctx = self.ctx;
        let mut inner = Writer::new(ctx, &mut *self.shared, self.level + 1, true);
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        block_pass(
            &refs,
            None,
            &mut own,
            true,
            Some(&mut |event| inner.project(event)),
            None,
        );
        Some(inner.finish())
    }
}
