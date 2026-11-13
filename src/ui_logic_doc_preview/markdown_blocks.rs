//! The streaming block reader: one pass collects definitions, the same pass renders with the complete map.

use super::markdown_container::View;
use super::markdown_document::{Shared, Writer};
use super::markdown_event::{Event, Frame, Lead, LineEvent, LineKind};
use super::markdown_html::document_text;
use super::markdown_model::{Block, FigureKind};
use super::markdown_refs::{Lines, Prefix, Refs};
use super::markdown_text::{at, is_space, trim};
use super::Ctx;
use std::cell::Cell;

const MIN_RULE_MARKS: usize = 3;
const MAX_RULE_INDENT: usize = 3;
const LIST_INTERRUPT_START: u64 = 1;
/// A working parse proves it every few thousand block-pass events or PROGRESS_MS, so a waiting pane waits for silence, never speed.
pub const PROGRESS_EVENTS: usize = 4000;
/// Events cost more or less as the box is loaded, so a parse also beats once PROGRESS_MS passed, the clock read every CLOCK_EVENTS events.
pub const PROGRESS_MS: u64 = 500;
pub const CLOCK_EVENTS: usize = 64;

thread_local! {
    /// Lines the block reader visited on this thread, which the count gates read to prove where and how often a document is parsed.
    pub static PASS_LINES: Cell<usize> = const { Cell::new(0) };
}

/// What watches a pass: its heartbeat listener and clock, and the test that stops it between two lines.
pub struct Watch<'w> {
    pub beat: Option<&'w mut dyn FnMut()>,
    pub clock: &'w dyn Fn() -> u64,
    pub stop: Option<&'w dyn Fn() -> bool>,
}

/// "mermaid chart title" selects a diagram by its first word, any case; "math" and "latex" a formula.
pub fn figure_kind(info: &str) -> Option<FigureKind> {
    let first = trim(info)
        .split(is_space)
        .next()
        .unwrap_or("")
        .to_lowercase();
    match first.as_str() {
        "mermaid" => Some(FigureKind::Mermaid),
        "math" | "latex" => Some(FigureKind::Math),
        _ => None,
    }
}

/// The thematic suffix of a line, cached before any nested container opens: "> - - -" has one starting at 2 with 3 marks.
pub fn rule_suffix(line: &str) -> (usize, usize) {
    let mut end = line.len();
    while end > 0 && matches!(at(line, end - 1), b' ' | b'\t') {
        end -= 1;
    }
    let tick = if end > 0 { at(line, end - 1) } else { 0 };
    if !matches!(tick, b'-' | b'*' | b'_') {
        return (line.len(), 0);
    }
    let mut count = 0;
    while end > 0 && (at(line, end - 1) == tick || matches!(at(line, end - 1), b' ' | b'\t')) {
        if at(line, end - 1) == tick {
            count += 1;
        }
        end -= 1;
    }
    (end, count)
}

// "## x" or "#" opens a heading line: up to three spaces, one to six hashes, then a blank or the end.
fn heading_line(text: &str) -> bool {
    let i = text.bytes().take(3).take_while(|b| *b == b' ').count();
    let hashes = text[i..].bytes().take_while(|b| *b == b'#').count();
    (1..=6).contains(&hashes)
        && super::markdown_text::char_at(text, i + hashes).is_none_or(is_space)
}

// "  ===" read lazily keeps its mark as text: a backslash goes before the first "=" or "-" after the leading whitespace.
fn escape_setext(text: &str) -> String {
    let lead = text.len() - text.trim_start_matches(is_space).len();
    match at(text, lead) {
        b'=' | b'-' => format!("{}\\{}", &text[..lead], &text[lead..]),
        _ => text.to_string(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Leaf {
    Paragraph {
        owner: usize,
    },
    Fence {
        owner: usize,
        tick: u8,
        len: usize,
        indent: usize,
    },
    Code,
}

enum Pending {
    Note {
        owner: usize,
        id: Option<String>,
        body: Vec<String>,
    },
    Ref {
        owner: usize,
        key: String,
        index: usize,
    },
}

impl Pending {
    fn owner(&self) -> usize {
        match self {
            Pending::Note { owner, .. } | Pending::Ref { owner, .. } => *owner,
        }
    }
}

fn finish_note(pending: &mut Option<Pending>, refs: &mut Option<&mut Refs>) {
    if let (
        Some(Pending::Note {
            id: Some(id), body, ..
        }),
        Some(refs),
    ) = (pending.take(), refs.as_deref_mut())
    {
        refs.notes.insert(id, body.join("\n"));
    }
}

fn prefixes(frames: &[Frame]) -> Vec<Prefix> {
    frames
        .iter()
        .map(|frame| {
            if frame.quote {
                Prefix::Quote
            } else {
                Prefix::Item(frame.content_col)
            }
        })
        .collect()
}

struct Sender<'e, 's, 'w> {
    emit: Option<&'e mut dyn FnMut(Event)>,
    watch: Option<&'s mut Watch<'w>>,
    since: usize,
    beat_at: u64,
}

impl Sender<'_, '_, '_> {
    fn count(&mut self) {
        let Some(watch) = self.watch.as_deref_mut() else {
            return;
        };
        let Some(beat) = watch.beat.as_deref_mut() else {
            return;
        };
        // A pass carrying the heartbeat proves it is moving every few thousand sends or every half second.
        self.since += 1;
        if self.since >= PROGRESS_EVENTS
            || (self.since.is_multiple_of(CLOCK_EVENTS)
                && (watch.clock)().saturating_sub(self.beat_at) >= PROGRESS_MS)
        {
            self.since = 0;
            self.beat_at = (watch.clock)();
            beat();
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn line(
        &mut self,
        kind: LineKind,
        index: usize,
        text: String,
        info: &str,
        chain: &[Frame],
        lead: &Lead,
        raw: &str,
        state: &Lines,
    ) {
        if let Some(emit) = self.emit.as_deref_mut() {
            emit(Event::Line(LineEvent {
                kind,
                index,
                text,
                info: info.to_string(),
                figure: if kind == LineKind::FenceOpen {
                    figure_kind(info)
                } else {
                    None
                },
                chain,
                lead,
                raw,
                state,
            }));
        }
        self.count();
    }

    fn whole(&mut self, event: Event) {
        if let Some(emit) = self.emit.as_deref_mut() {
            emit(event);
        }
    }

    fn stopped(&self) -> bool {
        self.watch
            .as_deref()
            .and_then(|watch| watch.stop)
            .is_some_and(|stop| stop())
    }
}

// One pass's state across its lines: the open containers, the leaf block in progress and a definition still being read.
struct Pass<'p, 'e, 's, 'w> {
    lines: &'p [&'p str],
    refs: Option<&'p mut Refs>,
    collect: bool,
    state: &'p mut Lines,
    nested: bool,
    send: Sender<'e, 's, 'w>,
    frames: Vec<Frame>,
    leaf: Option<Leaf>,
    pending: Option<Pending>,
    serial: usize,
    last_quote: Option<usize>,
    lead: Lead,
}

// One line once its containers are matched and opened: the text they leave, its owner, and the open fence it continues.
struct Here<'p> {
    i: usize,
    raw: &'p str,
    view: View,
    text: String,
    owner: usize,
    top: bool,
    setext: bool,
    fence: Option<(u8, usize, usize)>,
}

#[path = "markdown_blocks_frames.rs"]
mod frames;
#[path = "markdown_blocks_leaves.rs"]
mod leaves;

/// One block pass over the lines: with refs it collects definitions and notes, with emit it renders; false when watch stopped it.
/// "- > ```\n  > [id]: literal\n  > ```" matches an item, then a quote, before its fence; nested lines hold no front matter.
pub fn block_pass(
    lines: &[&str],
    refs: Option<&mut Refs>,
    state: &mut Lines,
    nested: bool,
    emit: Option<&mut dyn FnMut(Event)>,
    watch: Option<&mut Watch>,
) -> bool {
    let beat_at = watch.as_deref().map_or(0, |watch| (watch.clock)());
    let mut pass = Pass {
        lines,
        collect: refs.is_some(),
        refs,
        state,
        nested,
        send: Sender {
            emit,
            watch,
            since: 0,
            beat_at,
        },
        frames: Vec::new(),
        leaf: None,
        pending: None,
        serial: 0,
        last_quote: None,
        lead: Lead::default(),
    };
    let mut i = 0;
    while i < lines.len() {
        if pass.send.stopped() {
            return false;
        }
        PASS_LINES.with(|count| count.set(count.get() + 1));
        // Containers nested past the limit end the pass; the document then shows its source.
        let Some(here) = pass.containers(i) else {
            return true;
        };
        i = pass.leaf_line(here);
    }
    finish_note(&mut pass.pending, &mut pass.refs);
    true
}

fn system_clock() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64)
}

/// The clock a pass reads when its caller gives none.
pub const SYSTEM_CLOCK: &dyn Fn() -> u64 = &system_clock;

/// A document's blocks, the first `head` handed to on_head once written; None when the watch stopped the parse.
pub fn parse(
    source: &str,
    ctx: &Ctx,
    head: usize,
    on_head: &mut dyn FnMut(&[Block]),
    mut watch: Option<&mut Watch>,
) -> Option<Vec<Block>> {
    let text = document_text(source);
    let lines: Vec<&str> = text.split('\n').collect();
    let mut refs = Refs::default();
    let mut state = Lines::default();
    // The collecting pass carries the heartbeat too, so a slow reference scan still proves it is moving.
    if !block_pass(
        &lines,
        Some(&mut refs),
        &mut state,
        false,
        None,
        watch.as_deref_mut(),
    ) {
        return None;
    }
    if state.deep {
        return Some(super::deep_blocks());
    }
    let mut shared = Shared {
        refs: &mut refs,
        tokens: Vec::new(),
        cited: Vec::new(),
    };
    let mut writer = Writer::new(ctx, &mut shared, 0, false);
    let mut sent = head == 0;
    let mut project = |event: Event| {
        writer.project(event);
        // Written blocks are final, so the head handed over early is the head the whole parse answers.
        if !sent && writer.out.len() >= head {
            sent = true;
            on_head(&writer.out[..head]);
        }
    };
    if !block_pass(&lines, None, &mut state, false, Some(&mut project), watch) {
        return None;
    }
    Some(writer.finish())
}

/// The whole document's blocks.
pub fn blocks(
    source: &str,
    ctx: &Ctx,
    head: usize,
    on_head: &mut dyn FnMut(&[Block]),
) -> Vec<Block> {
    parse(source, ctx, head, on_head, None).unwrap_or_default()
}

/// True when the text nests containers past the limit, read by a collecting pass alone.
pub fn nests_too_deep(text: &str) -> bool {
    let text = document_text(text);
    let lines: Vec<&str> = text.split('\n').collect();
    let mut state = Lines::default();
    block_pass(
        &lines,
        Some(&mut Refs::default()),
        &mut state,
        false,
        None,
        None,
    );
    state.deep
}
