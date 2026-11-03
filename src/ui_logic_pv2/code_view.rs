//! The code view's lines: one file or text, coloured by the backend a screen and a margin at a time (ui/CodeLines.qml).
//!
//! The view holds only the lines it was answered, bounded to a few windows, and asks for the ones in view plus a
//! margin; an answer for any other token is dropped, and binary, large and failed are the file's whole answer.
use std::collections::BTreeMap;

/// The most lines one ask names (docs/protocol.md "code"); a longer window is asked in parts.
pub const LINE_CAP: usize = 400;
/// The byte gate every text kind shares: past it the frame is refused and nothing is asked.
pub const BYTE_GATE: u64 = 1_048_576;
/// Network and phone storage read the file here, so the gate is what one read holds whole.
pub const REMOTE_GATE: u64 = 262_144;
/// The most lines held at once; the ones farthest from the view go first.
pub const HELD_CAP: usize = 4 * LINE_CAP;

/// One line as (role, text) runs, tabs still raw; an empty line is no runs.
pub type Runs = Vec<(i64, String)>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Answer {
    #[default]
    Pending,
    Styled,
    Plain,
    Binary,
    Large,
    Failed,
}

impl Answer {
    /// The reply's state word; an unknown word is a failed read, never a partial one.
    pub fn parse(state: &str) -> Self {
        match state {
            "styled" => Answer::Styled,
            "plain" => Answer::Plain,
            "binary" => Answer::Binary,
            "large" => Answer::Large,
            _ => Answer::Failed,
        }
    }

    pub fn settled(self) -> bool {
        matches!(self, Answer::Styled | Answer::Plain)
    }

    /// Binary, large and failed end the file: nothing more is asked until a new view starts.
    pub fn terminal(self) -> bool {
        matches!(self, Answer::Binary | Answer::Large | Answer::Failed)
    }
}

/// What an ask carries: the path for the backend to open, or the bytes the caller read with the name that picks the language.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    Path(String),
    Text { text: String, name: String },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Ask {
    pub source: Source,
    pub first: usize,
    pub count: usize,
}

/// One `code` reply, already parsed off the wire.
#[derive(Clone, Debug, PartialEq)]
pub struct Reply {
    pub token: u64,
    pub state: Answer,
    pub first: usize,
    pub total: usize,
    pub lines: Vec<Runs>,
}

/// The lines the view shows: the 1-based first line in view and how many rows of lines fit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
    pub top: usize,
    pub rows: usize,
}

#[derive(Default)]
pub struct Lines {
    pub path: String,
    /// True sends the bytes in `text` (None until the caller has read them) instead of the path.
    pub from_text: bool,
    pub text: Option<String>,
    pub active: bool,
    /// A backend that can colour; without one the caller's own text is drawn plain.
    pub engine: bool,
    /// The most lines drawn, 0 for every line.
    pub limit: usize,
    /// Screens asked past the view on each side.
    pub margin: usize,
    pub answer: Answer,
    pub total: usize,
    held: BTreeMap<usize, Runs>,
    /// The token of the ask in flight, 0 when none.
    pub token: u64,
    asking: Option<(usize, usize)>,
    /// An ask is due at the end of the turn, so a change of several inputs asks once.
    pub due: bool,
    pub retiring: bool,
    /// Asks sent, for the count tests.
    pub asks: usize,
    /// Lines parsed into runs from replies or the caller's text, for the count tests.
    pub lines_taken: usize,
}

impl Lines {
    pub fn new(margin: usize) -> Self {
        Lines {
            margin,
            ..Lines::default()
        }
    }

    fn reset(&mut self) {
        self.held.clear();
        self.total = 0;
        self.answer = Answer::Pending;
        self.token = 0;
        self.asking = None;
    }

    /// Starts a view of a new file or text, after any of path, text, source, activity or backend changed.
    pub fn fresh(&mut self) {
        self.reset();
        if !self.active {
            self.due = false;
            return;
        }
        if !self.engine && self.from_text && self.text.is_some() {
            self.answer_plain();
        } else {
            self.due = true;
        }
    }

    /// A new subject: the path, and for the remote route the bytes the caller read (None while the read is out).
    pub fn set_subject(&mut self, path: &str, from_text: bool, text: Option<String>) {
        self.path = path.to_string();
        self.from_text = from_text;
        self.text = text;
        self.fresh();
    }

    pub fn set_active(&mut self, active: bool) {
        if self.active != active {
            self.active = active;
            self.fresh();
        }
    }

    /// The caller's own lines, plain: "a\nb\n" holds lines 1 and 2, matching the engine's terminal-newline rule; the read gate bounds them.
    pub fn answer_plain(&mut self) {
        let text = self.text.as_deref().unwrap_or("");
        let mut rows: Vec<&str> = if text.is_empty() {
            Vec::new()
        } else {
            text.split('\n').collect()
        };
        if !rows.is_empty() && text.ends_with('\n') {
            rows.pop();
        }
        self.held.clear();
        for (i, row) in rows.iter().enumerate().take(if self.limit > 0 {
            self.limit
        } else {
            usize::MAX
        }) {
            let line = row.strip_suffix('\r').unwrap_or(row);
            self.held.insert(i + 1, vec![(0, line.to_string())]);
            self.lines_taken += 1;
        }
        self.total = rows.len();
        self.answer = Answer::Plain;
        self.due = false;
    }

    pub fn shown(&self) -> usize {
        if self.limit > 0 {
            self.total.min(self.limit)
        } else {
            self.total
        }
    }

    pub fn row(&self, line: usize) -> Option<&Runs> {
        self.held.get(&line)
    }

    pub fn held(&self) -> usize {
        self.held.len()
    }

    /// The lines the view needs held: those in it plus the margin, never past the file or the limit.
    pub fn needed(&self, view: Window) -> (usize, usize) {
        let rows = view.rows.max(1);
        let top = view.top.max(1);
        let mut to = top + rows * (1 + self.margin) - 1;
        if self.limit > 0 {
            to = to.min(self.limit);
        }
        if self.answer.settled() {
            to = to.min(self.total);
        }
        (top.saturating_sub(rows * self.margin).max(1), to)
    }

    pub fn covered(&self, from: usize, to: usize) -> bool {
        (from..=to).all(|line| self.held.contains_key(&line))
    }

    /// True once the lines in view are all held.
    pub fn first_screen(&self, view: Window) -> bool {
        let (from, to) = self.needed(view);
        self.answer.settled() && self.covered(from, to)
    }

    /// The next ask for this view, or None when nothing is missing, one is already out for it, or the file has ended.
    pub fn ask(&mut self, view: Window) -> Option<Ask> {
        self.due = false;
        if self.retiring || !self.active || !self.engine || view.rows == 0 || self.answer.terminal()
        {
            return None;
        }
        if self.from_text {
            match &self.text {
                None => return None,
                Some(text) if text.is_empty() => {
                    self.answer_plain();
                    return None;
                }
                Some(_) => {}
            }
        } else if self.path.is_empty() {
            return None;
        }
        let want = self.needed(view);
        if want.1 < want.0 || (self.answer.settled() && self.covered(want.0, want.1)) {
            return None;
        }
        if self.token != 0 && self.asking == Some(want) {
            return None;
        }
        self.asking = Some(want);
        let mut from = want.0;
        while from < want.1 && self.held.contains_key(&from) {
            from += 1;
        }
        let source = if self.from_text {
            let name = self.path.rsplit('/').next().unwrap_or("").to_string();
            Source::Text {
                text: self.text.clone().unwrap_or_default(),
                name,
            }
        } else {
            Source::Path(self.path.clone())
        };
        self.asks += 1;
        Some(Ask {
            source,
            first: from,
            count: LINE_CAP.min(want.1 - from + 1),
        })
    }

    /// The host's token for the ask just sent; an answer carrying any other is dropped.
    pub fn sent(&mut self, token: u64) {
        self.token = token;
    }

    /// One reply: true when it was this view's and was taken.
    pub fn answered(&mut self, reply: Reply) -> bool {
        if self.token == 0 || reply.token != self.token {
            return false;
        }
        self.token = 0;
        self.answer = reply.state;
        self.total = reply.total;
        for (i, runs) in reply.lines.into_iter().enumerate() {
            self.held.insert(reply.first + i, runs);
            self.lines_taken += 1;
        }
        self.bound();
        self.due = self.answer.settled();
        true
    }

    // Lines far from the window last asked go first, so a long scroll keeps memory flat.
    fn bound(&mut self) {
        let centre = self.asking.map_or(1, |(from, to)| (from + to) / 2);
        while self.held.len() > HELD_CAP {
            let (first, last) = (
                *self.held.keys().next().unwrap(),
                *self.held.keys().next_back().unwrap(),
            );
            let drop = if centre.abs_diff(first) >= centre.abs_diff(last) {
                first
            } else {
                last
            };
            self.held.remove(&drop);
        }
    }

    /// The plain text of the lines drawn, for the readers that check what is on screen; never called from paint.
    pub fn plain_lines(&self) -> Vec<String> {
        (1..=self.shown())
            .map(|line| {
                self.row(line).map_or_else(String::new, |runs| {
                    let pairs: Vec<(i64, &str)> = runs
                        .iter()
                        .map(|(role, text)| (*role, text.as_str()))
                        .collect();
                    super::code::plain(&pairs)
                })
            })
            .collect()
    }
}

/// The page step for a viewport at a line height of `row`: one screen less a line, so the last line stays on top as
/// context, by whole lines only and at least one, so a page lands on a line.
pub fn page_step(viewport: f32, row: f32) -> f32 {
    if row <= 0. {
        return viewport;
    }
    ((viewport / row).floor() - 1.).max(1.) * row
}

/// How a text frame is refused: over the gate by the listing's size or the backend's count, a failed read, or a binary file.
pub fn refusal(
    size: u64,
    max_bytes: u64,
    answer: Answer,
    read_failed: bool,
) -> Option<super::refusal::Refusal> {
    use super::refusal::Refusal;
    if size > max_bytes || answer == Answer::Large {
        Some(Refusal::TooLarge)
    } else if read_failed || answer == Answer::Failed {
        Some(Refusal::Unreadable)
    } else if answer == Answer::Binary {
        Some(Refusal::Binary)
    } else {
        None
    }
}

#[cfg(test)]
#[path = "code_view_tests.rs"]
mod tests;
