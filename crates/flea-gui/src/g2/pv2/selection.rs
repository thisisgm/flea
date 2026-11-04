//! Mouse selection over the existing shaped preview runs, without moving keyboard focus.
use gpui::{
    fill, point, px, size, App, Bounds, ClipboardItem, ContentMask, Global, Hsla, KeyDownEvent,
    LineLayout, MouseDownEvent, Pixels, Point, Window, WindowId,
};
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    ops::Range,
    rc::{Rc, Weak},
    sync::Arc,
};

#[path = "selection_hit.rs"]
mod hit;

#[path = "selection_text.rs"]
mod text;
pub use text::{Chunk, Lines};

/// The field selection recipe's accent wash, as the rename and path fields draw it.
const SELECTION: f32 = 0.24;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Position {
    pub chunk: usize,
    pub byte: usize,
}

#[derive(Default)]
struct State {
    anchor: Option<Position>,
    head: Position,
    dragging: bool,
    word: Option<Range<Position>>,
    copied: Option<String>,
    copy_requested: bool,
    pending: bool,
}

#[derive(Default)]
struct Active(HashMap<WindowId, Weak<RefCell<State>>>);
impl Global for Active {}

/// One drawn piece. Only the visible glyph layouts are retained for hit testing.
pub struct Span {
    pub chunk: usize,
    pub start: usize,
    pub len: usize,
    pub bounds: Bounds<Pixels>,
    pub line: Arc<LineLayout>,
    pub clip: Bounds<Pixels>,
    /// Where the next piece of the same line starts, so a range running on washes the gap between them.
    pub reach: Pixels,
}

#[derive(Default)]
pub struct Selection {
    state: Rc<RefCell<State>>,
    pub spans: Vec<Span>,
    pub clip: Bounds<Pixels>,
    pub chunks: BTreeMap<usize, Chunk>,
    pub lines: Option<Lines>,
    pub contiguous: bool,
    pub gaps: Vec<Range<usize>>,
}

impl Selection {
    pub fn clear(&mut self) {
        *self.state.borrow_mut() = State::default();
        self.chunks.clear();
        self.lines = None;
        self.gaps.clear();
    }

    /// Marks the range's text as still on its way, so Copy waits for it instead of taking what is drawn.
    pub fn set_pending(&self, pending: bool) {
        self.state.borrow_mut().pending = pending;
    }

    /// True while a range's text is on its way or a Copy is waiting for it.
    pub fn waiting(&self) -> bool {
        let s = self.state.borrow();
        s.pending || s.copy_requested
    }

    fn chunk(&self, id: usize) -> Option<std::borrow::Cow<'_, Chunk>> {
        match &self.lines {
            Some(lines) => Some(std::borrow::Cow::Owned(Chunk::new(
                lines.raw(id)?.to_owned(),
            ))),
            None => self.chunks.get(&id).map(std::borrow::Cow::Borrowed),
        }
    }

    pub fn range(&self) -> Option<Range<Position>> {
        let s = self.state.borrow();
        let anchor = s.anchor?;
        (anchor != s.head).then(|| anchor.min(s.head)..anchor.max(s.head))
    }

    pub fn dragging(&self) -> bool {
        self.state.borrow().dragging
    }

    pub fn press(
        &mut self,
        event: &MouseDownEvent,
        at: Point<Pixels>,
        window: &Window,
        cx: &mut App,
    ) {
        let Some(hit) = self.hit(at) else {
            self.clear();
            return;
        };
        let previous = cx
            .try_global::<Active>()
            .and_then(|a| a.0.get(&window.window_handle().window_id()))
            .and_then(Weak::upgrade);
        if let Some(previous) = previous.filter(|s| !Rc::ptr_eq(s, &self.state)) {
            *previous.borrow_mut() = State::default();
        }
        let mut s = self.state.borrow_mut();
        if event.click_count == 2 {
            let word = self.word_under(at, hit);
            s.anchor = Some(word.start);
            s.head = word.end;
            s.word = Some(word);
        } else {
            if !event.modifiers.shift || s.anchor.is_none() {
                s.anchor = Some(hit);
            }
            s.head = hit;
            s.word = None;
        }
        s.dragging = true;
        s.copy_requested = false;
        s.copied = None;
        drop(s);
        self.publish(window, cx);
    }

    pub fn motion(&mut self, at: Point<Pixels>, window: &Window, cx: &mut App) {
        if !self.dragging() {
            return;
        }
        if let Some(hit) = self.hit(at) {
            let word = self.state.borrow().word.clone();
            let head = if let Some(word) = &word {
                if hit < word.start {
                    self.word(hit).start
                } else {
                    self.word(hit).end
                }
            } else {
                hit
            };
            let mut state = self.state.borrow_mut();
            if let Some(word) = word {
                state.anchor = Some(if hit < word.start {
                    word.end
                } else {
                    word.start
                });
            }
            if state.head != head {
                state.copy_requested = false;
                state.copied = None;
            }
            state.head = head;
            drop(state);
            self.publish(window, cx);
        }
    }

    /// Ends a drag and takes the range's text once, for a Copy now or one asked during the drag.
    pub fn release(&mut self, cx: &mut App) {
        let dragged = std::mem::take(&mut self.state.borrow_mut().dragging);
        if dragged {
            self.complete_copy(cx);
        }
    }

    pub fn text(&self) -> Option<String> {
        let range = self.range()?;
        if self.state.borrow().pending
            || self
                .gaps
                .iter()
                .any(|gap| range.start.chunk < gap.end && gap.start <= range.end.chunk)
        {
            return None;
        }
        let mut out = String::new();
        if let Some(lines) = &self.lines {
            for line in range.start.chunk..=range.end.chunk {
                if line != range.start.chunk && line != range.end.chunk {
                    out.push_str(lines.raw(line)?);
                    continue;
                }
                let chunk = Chunk::new(lines.raw(line)?.to_owned());
                out.push_str(chunk.raw.get(chunk.raw_span(line, &range))?);
            }
            return (!out.is_empty()).then_some(out);
        }
        let mut next = range.start.chunk;
        for (&id, chunk) in self.chunks.range(range.start.chunk..=range.end.chunk) {
            if self.contiguous && id != next {
                return None;
            }
            next = id + 1;
            out.push_str(chunk.raw.get(chunk.raw_span(id, &range))?);
        }
        if self.contiguous && next != range.end.chunk + 1 {
            return None;
        }
        (!out.is_empty()).then_some(out)
    }

    /// Takes the range's text; a waiting Copy is written once it settles, or dropped when no text came.
    pub fn complete_copy(&self, cx: &mut App) {
        let copied = self.text();
        let mut state = self.state.borrow_mut();
        state.copied = copied;
        if state.copy_requested && !state.pending && !state.dragging {
            state.copy_requested = false;
            if let Some(text) = &state.copied {
                cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
            }
        }
    }

    fn publish(&self, window: &Window, cx: &mut App) {
        if cx.try_global::<Active>().is_none() {
            cx.set_global(Active::default());
        }
        cx.global_mut::<Active>().0.insert(
            window.window_handle().window_id(),
            Rc::downgrade(&self.state),
        );
    }

    /// Where a piece's wash ends when the range runs on past it: the start of the line's next piece.
    pub fn wash_end(&self, span: &Span, part: Range<usize>) -> Option<Pixels> {
        let range = self.range()?;
        let end = Position {
            chunk: span.chunk,
            byte: span.start + span.len,
        };
        (part.end == span.len && range.end > end && span.reach > span.bounds.right())
            .then_some(span.reach)
    }

    /// The selected part of a shaped piece, in its byte offsets.
    pub fn part(&self, chunk: usize, start: usize, len: usize) -> Option<Range<usize>> {
        let range = self.range()?;
        if chunk < range.start.chunk || chunk > range.end.chunk {
            return None;
        }
        let from = if chunk == range.start.chunk {
            range.start.byte
        } else {
            0
        };
        let to = if chunk == range.end.chunk {
            range.end.byte
        } else {
            usize::MAX
        };
        let end = start + len;
        let (from, to) = (from.max(start), to.min(end));
        (from < to).then(|| from - start..to - start)
    }
}

/// A window losing the front drops a waiting Copy, so text that lands later never overwrites another app's copy.
pub fn blur(window: &Window, cx: &mut App) {
    if let Some(state) = cx
        .try_global::<Active>()
        .and_then(|a| a.0.get(&window.window_handle().window_id()))
        .and_then(Weak::upgrade)
    {
        state.borrow_mut().copy_requested = false;
    }
}

pub fn clear_window(window: &mut Window, cx: &mut App) {
    if let Some(state) = cx
        .try_global::<Active>()
        .and_then(|a| a.0.get(&window.window_handle().window_id()))
        .and_then(Weak::upgrade)
    {
        *state.borrow_mut() = State::default();
        window.refresh();
    }
}

/// Returns true only for a preview range's Copy or Escape; other keys keep their current owner.
pub fn key(event: &KeyDownEvent, window: &mut Window, cx: &mut App) -> bool {
    let Some(state) = cx
        .try_global::<Active>()
        .and_then(|a| a.0.get(&window.window_handle().window_id()))
        .and_then(Weak::upgrade)
    else {
        return false;
    };
    let mut state = state.borrow_mut();
    let mods = event.keystroke.modifiers;
    if event.keystroke.key == "escape" && !mods.modified() && state.anchor.is_some() {
        let selected = state.anchor.is_some_and(|anchor| anchor != state.head);
        *state = State::default();
        window.refresh();
        if selected {
            cx.stop_propagation();
            return true;
        }
    }
    if event.keystroke.key.eq_ignore_ascii_case("c")
        && mods.control
        && !mods.alt
        && !mods.platform
        && !mods.shift
    {
        if let Some(text) = &state.copied {
            cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
            cx.stop_propagation();
            return true;
        }
        let selected = state.anchor.is_some_and(|anchor| anchor != state.head);
        if selected && (state.pending || state.dragging) {
            state.copy_requested = true;
            cx.stop_propagation();
            return true;
        }
    }
    false
}

/// Paint the field selection recipe over the original run, keeping its shaping and clip.
pub fn paint(
    run: &gpui::GlyphRun,
    origin: Point<Pixels>,
    band: Bounds<Pixels>,
    selected: Range<usize>,
    reach: Option<Pixels>,
    (accent, foreground): (Hsla, Hsla),
    window: &mut Window,
) {
    let line = run.line();
    let from = origin.x + line.x_for_index(selected.start);
    let to = (origin.x + line.x_for_index(selected.end)).max(reach.unwrap_or(px(0.)));
    let bounds = Bounds::new(point(from, band.top()), size(to - from, band.size.height));
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        window.paint_quad(fill(bounds, accent.opacity(SELECTION)));
        let _ = window.paint_glyph_run(origin, run, foreground);
    });
}

#[cfg(test)]
#[path = "selection_tests.rs"]
mod tests;
