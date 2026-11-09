//! The code view's two bounded caches: each held line prepared once (tabs filled, wrapped or cut, as byte ranges), and
//! each row in view shaped once into glyph runs per coloured piece; the ink is chosen at paint, so a theme change
//! reshapes nothing, and a line wrapped into thousands of rows shapes only the rows the view reaches.
use flea::ui_logic_doc_preview::{code, code_text};
use gpui::{Font, GlyphRun, Hsla, Pixels, TextRun, Window};
use std::{cell::Cell, collections::HashMap, hash::Hash, ops::Range, rc::Rc};

thread_local! {
    // Bytes handed to the shaper on this thread, for the count tests.
    static SHAPED_BYTES: Cell<usize> = const { Cell::new(0) };
}

/// The bytes shaped on this thread so far, for the count tests.
pub fn shaped_bytes() -> usize {
    SHAPED_BYTES.with(Cell::get)
}

/// One coloured piece of a row: its ink index (0 the foreground, 1 to 6 bat's colours) and its x from the text's left.
pub struct Piece {
    pub start: usize,
    pub len: usize,
    pub ink: usize,
    pub x: Pixels,
    pub run: GlyphRun,
}

/// One row of a line as shaped, and the gutter number on the line's first row.
pub struct Shaped {
    pub line: usize,
    pub pieces: Vec<Piece>,
    pub number: Option<GlyphRun>,
}

/// One line laid out once: its text with tabs filled, where each coloured piece starts, and each row's byte range.
pub struct Prepared {
    pub text: String,
    pub pieces: Vec<(usize, usize)>,
    pub rows: Vec<Range<usize>>,
    /// The unwrapped line was cut short and its row ends in an ellipsis.
    pub cut: bool,
}

/// What every cached row was shaped for; any change drops them all.
#[derive(Clone, Debug, PartialEq)]
pub struct Shaping {
    pub font: Font,
    pub body: Pixels,
    pub caption: Pixels,
    pub cells: usize,
    pub wrap: bool,
    pub numbered: bool,
    pub scale: f32,
}

/// A bounded LRU map; the entry used longest ago goes first.
pub struct Lru<K, V> {
    entries: HashMap<K, (V, u64)>,
    clock: u64,
    capacity: usize,
    pub made: u64,
}

impl<K: Copy + Eq + Hash, V: Clone> Default for Lru<K, V> {
    fn default() -> Self {
        Lru {
            entries: HashMap::new(),
            clock: 0,
            capacity: 1,
            made: 0,
        }
    }
}

impl<K: Copy + Eq + Hash, V: Clone> Lru<K, V> {
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn peek(&self, key: &K) -> Option<&V> {
        self.entries.get(key).map(|(value, _)| value)
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.entries.iter().map(|(key, (value, _))| (key, value))
    }

    pub fn bound(&mut self, capacity: usize) {
        self.capacity = capacity.max(1);
        while self.entries.len() > self.capacity {
            self.evict();
        }
    }

    pub fn get_or_make(&mut self, key: K, make: impl FnOnce() -> V) -> V {
        self.clock += 1;
        if let Some((value, used)) = self.entries.get_mut(&key) {
            *used = self.clock;
            return value.clone();
        }
        if self.entries.len() >= self.capacity {
            self.evict();
        }
        self.made += 1;
        let value = make();
        self.entries.insert(key, (value.clone(), self.clock));
        value
    }

    fn evict(&mut self) {
        let oldest = self
            .entries
            .iter()
            .min_by_key(|(_, (_, used))| *used)
            .map(|(key, _)| *key);
        if let Some(oldest) = oldest {
            self.entries.remove(&oldest);
        }
    }
}

/// The shaped rows in view, keyed by (line, row), cleared whenever the shaping changes.
#[derive(Default)]
pub struct RowCache {
    shaping: Option<Shaping>,
    pub rows: Lru<(usize, usize), Rc<Shaped>>,
}

impl RowCache {
    pub fn clear(&mut self) {
        self.rows.clear();
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Rows shaped since the view was built, for the count tests.
    pub fn shaped(&self) -> u64 {
        self.rows.made
    }

    /// Starts a frame: a new shaping clears the rows, and the bound follows the viewport.
    pub fn begin(&mut self, shaping: &Shaping, capacity: usize) {
        if self.shaping.as_ref() != Some(shaping) {
            self.rows.clear();
            self.shaping = Some(shaping.clone());
        }
        self.rows.bound(capacity);
    }
}

/// Shapes one text in the face at a size; the colour is given at paint.
pub fn glyphs(window: &Window, text: &str, font: &Font, size: Pixels) -> GlyphRun {
    SHAPED_BYTES.with(|bytes| bytes.set(bytes.get() + text.len()));
    let run = TextRun {
        len: text.len(),
        font: font.clone(),
        color: Hsla::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    GlyphRun::new(window.text_system().layout_line(text, size, &[run], None))
}

/// Where a text too wide for its line is cut: the end (an entry, a facts line) or the middle (an archive's name).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Elide {
    End,
    Middle,
}

/// The text as it fits `width`, cut with an ellipsis where it does not (Text.ElideRight or ElideMiddle), shaped.
/// Only a head (and for the middle cut a tail) a little wider than the room is ever shaped, never the whole text.
pub fn elided(
    window: &Window,
    text: &str,
    font: &Font,
    size: Pixels,
    width: Pixels,
    elide: Elide,
) -> (String, GlyphRun) {
    // A first guess at the characters that fit, from an advance of a quarter of the size; it doubles until it is wrong.
    let guess = (f32::from(width) / (f32::from(size) * 0.25).max(1.)).ceil() as usize + 1;
    let (end, head) = grown(guess, |chars| {
        let end = text
            .char_indices()
            .nth(chars)
            .map_or(text.len(), |(at, _)| at);
        let run = glyphs(window, &text[..end], font, size);
        let done = end == text.len() || run.line().width > width;
        (end, run, done)
    });
    if end == text.len() && head.line().width <= width {
        return (text.to_owned(), head);
    }
    let ellipsis = glyphs(window, "\u{2026}", font, size).line().width;
    let next = |at: usize| at + text[at..].chars().next().map_or(0, char::len_utf8);
    let kept = match elide {
        Elide::End => {
            let room = (width - ellipsis).max(Pixels::ZERO);
            let cut = head.line().index_for_x(room).unwrap_or(end);
            format!("{}\u{2026}", &text[..cut])
        }
        Elide::Middle => {
            let room = ((width - ellipsis) / 2.).max(Pixels::ZERO);
            let cut = head.line().index_for_x(room).unwrap_or(end);
            let (start, tail) = grown(guess, |chars| {
                let start = text.char_indices().rev().nth(chars).map_or(0, |(at, _)| at);
                let run = glyphs(window, &text[start..], font, size);
                let done = start <= cut || run.line().width > room;
                (start, run, done)
            });
            let line = tail.line();
            let from = line
                .index_for_x(line.width - room)
                .map_or(text.len(), |at| next(start + at))
                .max(cut);
            format!("{}\u{2026}{}", &text[..cut], &text[from..])
        }
    };
    let run = glyphs(window, &kept, font, size);
    (kept, run)
}

// Shapes a growing piece of a text, doubling its characters until `shape` says it is done.
fn grown(mut chars: usize, shape: impl Fn(usize) -> (usize, GlyphRun, bool)) -> (usize, GlyphRun) {
    loop {
        let (at, run, done) = shape(chars);
        if done {
            return (at, run);
        }
        chars = chars.saturating_mul(2);
    }
}

/// Lays one line out at `cells` per row: tabs filled, wrapped, or cut to leave a cell for the ellipsis. One pass.
pub fn prepare(runs: &[(i64, String)], cells: usize, wrap: bool) -> Prepared {
    let raw: Vec<(i64, &str)> = runs
        .iter()
        .map(|(role, text)| (*role, text.as_str()))
        .collect();
    let mut text = String::new();
    let mut pieces = Vec::new();
    for (role, piece) in code::pairs(&raw) {
        if !piece.is_empty() {
            pieces.push((ink_index(role), text.len()));
            text.push_str(&piece);
        }
    }
    let (spans, cut) = if wrap {
        (code_text::wrap(&text, cells), false)
    } else {
        match code_text::elide(&text, cells) {
            Some(cut) => (std::iter::once(0..cut).collect(), true),
            None => (std::iter::once(0..text.chars().count()).collect(), false),
        }
    };
    let rows = bytes_of(&text, spans);
    Prepared {
        text,
        pieces,
        rows,
        cut,
    }
}

// Char ranges in rising order to byte ranges, walking the text once.
fn bytes_of(text: &str, spans: Vec<Range<usize>>) -> Vec<Range<usize>> {
    let end = text.len();
    let mut starts = text.char_indices().map(|(at, _)| at).peekable();
    let mut seen = 0;
    let mut byte = |char_at: usize| {
        while seen < char_at {
            starts.next();
            seen += 1;
        }
        starts.peek().copied().unwrap_or(end)
    };
    spans
        .into_iter()
        .map(|span| byte(span.start)..byte(span.end))
        .collect()
}

/// Shapes row `row` of a prepared line; the line's first row carries its 1-based gutter number.
pub fn shape_row(
    window: &Window,
    shaping: &Shaping,
    prepared: &Prepared,
    row: usize,
    number: usize,
) -> Shaped {
    let range = prepared.rows[row].clone();
    let first = prepared
        .pieces
        .partition_point(|(_, start)| *start <= range.start)
        .saturating_sub(1);
    let mut pieces = Vec::new();
    let mut x = Pixels::ZERO;
    for (index, (ink, start)) in prepared.pieces.iter().enumerate().skip(first) {
        if *start >= range.end {
            break;
        }
        let end = prepared
            .pieces
            .get(index + 1)
            .map_or(prepared.text.len(), |(_, next)| *next);
        let (from, to) = ((*start).max(range.start), end.min(range.end));
        if from < to {
            let run = glyphs(
                window,
                &prepared.text[from..to],
                &shaping.font,
                shaping.body,
            );
            let width = run.line().width;
            pieces.push(Piece {
                start: from,
                len: to - from,
                ink: *ink,
                x,
                run,
            });
            x += width;
        }
    }
    if prepared.cut {
        let run = glyphs(window, "\u{2026}", &shaping.font, shaping.body);
        let ink = pieces.last().map_or(0, |piece| piece.ink);
        pieces.push(Piece {
            start: range.end,
            len: 0,
            ink,
            x,
            run,
        });
    }
    let gutter = (shaping.numbered && row == 0)
        .then(|| glyphs(window, &number.to_string(), &shaping.font, shaping.caption));
    if pieces.is_empty() {
        pieces.push(Piece {
            start: range.start,
            len: 0,
            ink: 0,
            x,
            run: glyphs(window, "", &shaping.font, shaping.body),
        });
    }
    Shaped {
        line: number.saturating_sub(1),
        pieces,
        number: gutter,
    }
}

/// A role's ink slot; 0 and any unknown role are the foreground.
pub fn ink_index(role: i64) -> usize {
    if (1..=code::INKS.len() as i64).contains(&role) {
        role as usize
    } else {
        0
    }
}
