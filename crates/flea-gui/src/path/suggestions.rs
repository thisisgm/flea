//! The path bar's folder jump: one backend ask per open, the rows a name typed into the bar ranks, and the popup they fill.
use super::{scale::Scale, Jumped};

/// One suggestion: the path as the chrome writes it, where its leaf starts, and the typed match run, in bytes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Suggestion {
    pub path: String,
    pub text: String,
    pub leaf_start: usize,
    pub wash: std::ops::Range<usize>,
}

/// The ranking seam: the scorer, Jump.js's port, is the focus and selection unit's, and plugs in here.
pub trait Ranker {
    /// Folds one answer once, so every keystroke ranks out of it instead of folding each candidate again.
    fn prepare(&mut self, answer: &Jumped, home: &str);
    /// The rows a line ranks, best first, written into out.
    fn rows(&self, line: &str, out: &mut Vec<Suggestion>);
    /// Whether a line is a name to jump by rather than a path.
    fn is_query(&self, line: &str) -> bool;
    /// False for a ranker that ranks nothing, so an open spends no backend ask on it.
    fn ranks(&self) -> bool {
        true
    }
}

/// The seam's stand-in until the scorer lands: no rows, so the bar is the plain path bar and Enter commits the line.
pub struct Unranked;

impl Ranker for Unranked {
    fn prepare(&mut self, _: &Jumped, _: &str) {}
    fn rows(&self, _: &str, _: &mut Vec<Suggestion>) {}
    fn is_query(&self, _: &str) -> bool {
        false
    }
    fn ranks(&self) -> bool {
        false
    }
}

/// What the jump asks of its host: a jump line for the backend, a read of the recent history, or the bar's own answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ask {
    Jump { id: u64, ranking: u64 },
    ReadHistory,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Enter {
    Chosen(String),
    Declined,
    Held,
}

// The backend answers inside its own budgets; one that never answers must not hold an Enter for good.
pub const ANSWER_LIMIT_MS: u64 = 4000;

#[derive(Default)]
pub struct Jump {
    editing: bool,
    path_typed: bool,
    answered: bool,
    pub entries: Vec<Suggestion>,
    pub cursor: Option<usize>,
    pub high_water: usize,
    asked: u64,
    provisional: u64,
    whole_taken: bool,
    prov_taken: bool,
    whole_asked: bool,
    pub enter_waiting: bool,
    /// The recent history: kept until it changes, reading, and how many changes the kept copy was read at.
    pub history_kept: bool,
    history_reading: bool,
    pub history_changes: u64,
    history_read_at: Option<u64>,
}

impl Jump {
    pub fn shown(&self) -> bool {
        !self.entries.is_empty()
    }

    /// An open asks at once with what is kept, and a stale history asks twice: provisional now, whole after the read.
    pub fn open(&mut self, out: &mut Vec<Ask>) {
        self.reset(true);
        if self.history_kept && self.history_read_at == Some(self.history_changes) {
            self.whole_asked = true;
            out.push(Ask::Jump {
                id: self.asked,
                ranking: 0,
            });
        } else {
            self.provisional = self.asked;
            out.push(Ask::Jump {
                id: self.asked,
                ranking: 0,
            });
            self.history_reading = true;
            self.history_read_at = Some(self.history_changes);
            out.push(Ask::ReadHistory);
        }
    }

    pub fn close(&mut self) {
        self.reset(false);
    }

    fn reset(&mut self, editing: bool) {
        self.editing = editing;
        self.path_typed = false;
        self.answered = false;
        self.entries.clear();
        self.cursor = None;
        self.high_water = 0;
        self.provisional = 0;
        self.whole_taken = false;
        self.prov_taken = false;
        self.whole_asked = false;
        self.enter_waiting = false;
        self.asked += 1;
    }

    /// The read landed: the whole ask waits for the provisional answer as well, so two asks never share zoxide's slot.
    pub fn history_read(&mut self, out: &mut Vec<Ask>) {
        self.history_kept = true;
        self.history_reading = false;
        self.ask_whole(out);
    }

    fn ask_whole(&mut self, out: &mut Vec<Ask>) {
        if !self.editing || self.whole_asked || !self.prov_taken {
            return;
        }
        if self.provisional != 0 && !(self.history_kept && !self.history_reading) {
            return;
        }
        self.whole_asked = true;
        self.asked += 1;
        out.push(Ask::Jump {
            id: self.asked,
            ranking: self.provisional,
        });
    }

    /// The backend's answer; one for another open, or after the bar closed, is dropped. A provisional answer never spends a held Enter.
    pub fn take(
        &mut self,
        answer: &Jumped,
        ranker: &mut dyn Ranker,
        home: &str,
        line: &str,
        out: &mut Vec<Ask>,
    ) -> Option<Enter> {
        if !self.editing {
            return None;
        }
        let provisional =
            answer.id == self.provisional && self.provisional != 0 && !self.whole_taken;
        if answer.id != self.asked && !provisional {
            return None;
        }
        ranker.prepare(answer, home);
        self.answered = true;
        self.rank(ranker, line);
        if provisional {
            self.prov_taken = true;
            self.ask_whole(out);
            return None;
        }
        self.whole_taken = true;
        if self.enter_waiting {
            self.enter_waiting = false;
            return Some(self.enter());
        }
        None
    }

    /// The line changed: rows rank again and the cursor returns to the first.
    pub fn rank(&mut self, ranker: &dyn Ranker, line: &str) {
        self.entries.clear();
        if self.editing && !self.path_typed && self.answered {
            ranker.rows(line, &mut self.entries);
        }
        self.high_water = self.high_water.max(self.entries.len());
        self.cursor = step(self.entries.len(), None, 1);
    }

    /// Tab is the bar's own completion, so a line it touched is a path for the rest of the edit.
    pub fn path_typed(&mut self) {
        self.path_typed = true;
        self.entries.clear();
        self.cursor = None;
    }

    pub fn step(&mut self, delta: isize) -> bool {
        let next = step(self.entries.len(), self.cursor, delta);
        let moved = next != self.cursor;
        self.cursor = next;
        moved
    }

    pub fn enter(&self) -> Enter {
        match self.cursor.and_then(|at| self.entries.get(at)) {
            Some(row) => Enter::Chosen(row.path.clone()),
            None => Enter::Declined,
        }
    }

    /// Enter: a shown row is chosen; a name typed before the whole answer is held so the same keys open the same folder.
    pub fn press_enter(&mut self, ranker: &dyn Ranker, line: &str) -> Option<Enter> {
        if self.shown() {
            return Some(self.enter());
        }
        if (!self.answered || !self.whole_taken) && !self.path_typed && ranker.is_query(line) {
            self.enter_waiting = true;
            return Some(Enter::Held);
        }
        None
    }

    /// The held Enter outlived the answer limit, so the line is a path after all.
    pub fn expire(&mut self) -> bool {
        std::mem::replace(&mut self.enter_waiting, false)
    }
}

// Jump.step: clamped, and None while nothing stands.
pub fn step(count: usize, from: Option<usize>, delta: isize) -> Option<usize> {
    if count == 0 {
        return None;
    }
    let from = from.map_or(-1, |at| at as isize);
    Some((from + delta).clamp(0, count as isize - 1) as usize)
}

// The popup's 1 px frame, the hairline the board draws at every text size.
pub const FRAME: f32 = 1.;

/// The popup's frame in window pixels, the scroll its rows have, and the text size's geometry it lays out with.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Popup {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub content: f32,
    pub inner: f32,
    pub scale: Scale,
}

impl Popup {
    pub fn measure(scale: Scale, window_height: f32, x: f32, width: f32, rows: usize) -> Self {
        let content = rows as f32 * scale.row + 2. * scale.pad;
        let room = (window_height - scale.top() - scale.chrome - scale.clear).max(0.);
        let height = (content + 2. * FRAME).min(room);
        Self {
            x,
            y: scale.top(),
            width,
            height,
            content,
            inner: (height - 2. * FRAME).max(0.),
            scale,
        }
    }

    /// The popup as it was laid out, its rows' content measured against the frame it got.
    pub fn framed(scale: Scale, x: f32, y: f32, width: f32, height: f32, rows: usize) -> Self {
        let content = rows as f32 * scale.row + 2. * scale.pad;
        Self {
            x,
            y,
            width,
            height,
            content,
            inner: (height - 2. * FRAME).max(0.),
            scale,
        }
    }

    /// The rows any part of which shows at this scroll.
    pub fn visible(&self, scroll: f32, rows: usize) -> (usize, usize) {
        let (row, pad) = (self.scale.row, self.scale.pad);
        let first = ((scroll - pad) / row).floor().max(0.) as usize;
        let last = ((scroll + self.inner - pad) / row).ceil().max(0.) as usize;
        (first.min(rows), last.min(rows))
    }

    pub fn max_scroll(&self) -> f32 {
        (self.content - self.inner).max(0.)
    }

    /// The scroll that shows row index whole; the first and last rows bring their padding with them.
    pub fn reveal(&self, scroll: f32, index: usize, rows: usize) -> f32 {
        let row = self.scale.row;
        let top = self.scale.pad + index as f32 * row;
        let low = if index == 0 { 0. } else { top };
        let high = if index + 1 >= rows {
            self.content
        } else {
            top + row
        };
        let scroll = if low < scroll {
            low
        } else if high > scroll + self.inner {
            high - self.inner
        } else {
            scroll
        };
        scroll.clamp(0., self.max_scroll())
    }

    pub fn fades(&self, scroll: f32) -> (bool, bool) {
        (scroll > 0., scroll + self.inner < self.content)
    }

    /// The row under a window point, if any.
    pub fn row_at(&self, x: f32, y: f32, scroll: f32, rows: usize) -> Option<usize> {
        if x < self.x
            || x >= self.x + self.width
            || y < self.y + FRAME
            || y >= self.y + self.height - FRAME
        {
            return None;
        }
        let inside = y - self.y - FRAME + scroll - self.scale.pad;
        let index = (inside / self.scale.row).floor();
        (inside >= 0. && (index as usize) < rows).then_some(index as usize)
    }

    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }
}

/// Rows appearing under a resting pointer never take the cursor: a move counts only away from where it last was.
#[derive(Default)]
pub struct Pointer {
    seen: Option<(f32, f32)>,
    pub settling: bool,
}

impl Pointer {
    pub fn appeared(&mut self) {
        self.seen = None;
        self.settling = true;
    }

    /// True when the pointer truly moved over the rows and the cursor may follow it.
    pub fn moved(&mut self, at: (f32, f32)) -> bool {
        let first = self.seen.is_none();
        let moved = self.seen != Some(at);
        self.seen = Some(at);
        !first && moved && !self.settling
    }

    /// The last point the popup has seen, in window coordinates, or None since it opened.
    pub fn last_seen(&self) -> Option<(f32, f32)> {
        self.seen
    }
}
