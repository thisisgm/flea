//! The CSV table's feed: the records a frame needs, the asks that fetch them, the held lines, the parsed table and its widths; a parse is a `Job` the caller may run on another thread.
use super::csv::{self, Held, Slot, Table};
pub use super::csv_geometry::{body_face, Face, Geometry};
pub use super::csv_wire::{ask_line, parse_reply, Answer, Ask, Reply, LINE_CAP};
use std::collections::BTreeMap;
use std::sync::Arc;

/// The table rule under the header, the table rule of the Markdown preview.
pub const RULE_OPACITY: f32 = 0.12;

/// What a feed did, for the deterministic cost tests and the native run.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Counters {
    pub asks: usize,
    pub replies: usize,
    pub parses: usize,
    pub lines_parsed: usize,
    pub measured: usize,
    /// Frames the view laid out, so a test reads the cost of each frame however many frames a draw runs.
    pub frames: usize,
}

/// A parse of the held lines, independent of the feed so it can run on another thread.
pub struct Job {
    held: Arc<BTreeMap<usize, Held>>,
    total: usize,
    delim: Option<char>,
}

impl Job {
    pub fn run(&self) -> Table {
        csv::table(&self.held, self.total, self.delim)
    }
}

/// The raw lines of the caller's own text, numbered from one, and their count.
pub fn plain_held(text: &str) -> (BTreeMap<usize, Held>, usize) {
    let lines = csv::plain_lines(text);
    let total = lines.len();
    let held = lines
        .into_iter()
        .enumerate()
        .map(|(i, t)| (i + 1, Held { t, record: None }))
        .collect();
    (held, total)
}

pub struct Feed {
    pub delim: Option<char>,
    pub geometry: Geometry,
    /// The most records shown, 0 for all.
    pub limit: usize,
    /// Screens asked past the view on each side.
    pub margin: usize,
    pub held: Arc<BTreeMap<usize, Held>>,
    pub total: usize,
    pub records: usize,
    pub answer: Answer,
    /// The ask in flight, 0 when none: a reply for any other token is dropped.
    pub token: u64,
    /// The last record of the first screen, so the first screen is known whole before the table is drawn.
    pub first_to: usize,
    pub table: Table,
    /// The first screen is whole: columns start from its rows, widths then grow to the widest held cell, alignments stay.
    pub frozen: bool,
    /// How many measured columns each held record, by ordinal, has widened.
    pub folded: BTreeMap<usize, usize>,
    /// A held cell may still widen its column, so `measure` has work even with every reached column measured.
    pub widening: bool,
    /// The table row the next widening starts at, so no frame walks again the rows an earlier frame finished.
    pub widen_from: usize,
    /// Bumped whenever a measured column widens, so a layout kept for the old widths is laid out again.
    pub revision: u64,
    pub heads: Vec<f64>,
    pub widths: Vec<f64>,
    pub rights: Vec<bool>,
    /// The right edge the measured columns reach, in the same terms as `measure`'s target: Quick Look's grid or the fit.
    pub reach: f64,
    /// A parse is out: no ask goes until it lands.
    pub parsing: bool,
    plain: bool,
    /// The frame's body rows and the record ordinal at its top.
    pub view: (usize, usize),
    pub counters: Counters,
}

impl Feed {
    pub fn new(delim: Option<char>, geometry: Geometry) -> Self {
        Feed {
            delim,
            geometry,
            limit: 0,
            margin: 1,
            held: Arc::new(BTreeMap::new()),
            total: 0,
            records: 0,
            answer: Answer::Waiting,
            token: 0,
            first_to: 0,
            table: csv::table(&BTreeMap::new(), 0, delim),
            frozen: false,
            folded: BTreeMap::new(),
            widening: false,
            widen_from: 0,
            revision: 0,
            heads: Vec::new(),
            widths: Vec::new(),
            rights: Vec::new(),
            reach: if geometry.look {
                geometry.number_width
            } else {
                0.
            },
            parsing: false,
            plain: false,
            view: (1, 2),
            counters: Counters::default(),
        }
    }

    /// A new file or text: everything held goes, and a reply still out for the old one is dropped by its token.
    pub fn reset(&mut self, delim: Option<char>) {
        let (geometry, limit, margin, view) = (self.geometry, self.limit, self.margin, self.view);
        *self = Feed {
            limit,
            margin,
            view,
            ..Feed::new(delim, geometry)
        };
    }

    pub fn settled(&self) -> bool {
        self.answer.settled()
    }

    pub fn shown(&self) -> usize {
        if self.limit > 0 {
            self.records.min(self.limit)
        } else {
            self.records
        }
    }

    /// One row per data record; continuation lines never take a row or a number.
    pub fn body_rows(&self) -> usize {
        csv::data_rows(self.shown())
    }

    /// The frame's body rows and the ordinal of its top record, which `needed` asks around.
    pub fn set_view(&mut self, rows: usize, top: usize) {
        self.view = (rows.max(1), top.max(2));
    }

    fn row_at(&self, n: usize) -> bool {
        if n == 1 {
            self.table.header.is_some()
        } else {
            self.table.at_record(n).is_some()
        }
    }

    pub fn covered(&self, from: usize, to: usize) -> bool {
        (from..=to).all(|n| self.row_at(n))
    }

    /// The record ordinals the view needs, with the margin, file and limit bounds; ordinal 1 is the header.
    pub fn needed(&self) -> (usize, usize) {
        let (rows, top) = self.view;
        let mut to = top + rows * (1 + self.margin) - 1;
        if self.limit > 0 {
            to = to.min(self.limit);
        }
        if self.settled() {
            to = to.min(self.records);
        }
        let from = if self.row_at(1) {
            2.max(top.saturating_sub(rows * self.margin))
        } else {
            1
        };
        (from, to)
    }

    fn first_last(&self) -> usize {
        self.shown().min(self.first_to)
    }

    /// The first screen is held whole, header included, and no record in it is still open.
    pub fn first_ready(&self) -> bool {
        let last = self.first_last();
        self.settled() && self.covered(1, last) && csv::ready(&self.table, last)
    }

    /// A frame may draw the table: Quick Look once its first screen is whole, the column once its own screen is too.
    pub fn first_screen(&self) -> bool {
        if !self.first_ready() {
            return false;
        }
        if self.geometry.look {
            return true;
        }
        let (from, to) = self.needed();
        self.covered(from, to) && csv::ready(&self.table, to)
    }

    /// The next ask the view needs, None while one is out, a parse is pending, or the file refused.
    pub fn ask(&mut self) -> Option<Ask> {
        if self.answer.refused() || self.token != 0 || self.parsing || self.plain {
            return None;
        }
        let (from, to) = self.needed();
        if to < from || (self.settled() && self.covered(from, to)) {
            return None;
        }
        let mut first = from;
        while first <= to && self.row_at(first) {
            first += 1;
        }
        if self.first_to == 0 {
            self.first_to = to;
        }
        self.counters.asks += 1;
        Some(Ask {
            record: first,
            first,
            count: (to + 1).saturating_sub(first).clamp(1, LINE_CAP),
        })
    }

    /// The engine's token for the ask just sent.
    pub fn sent(&mut self, token: u64) {
        self.token = token;
    }

    /// A reply: only the ask in flight takes it; its lines are held and a parse is wanted.
    pub fn answered(&mut self, reply: &Reply) -> bool {
        if self.token == 0 || reply.token != self.token {
            return false;
        }
        self.token = 0;
        if !self.settled() || reply.state.refused() {
            self.answer = reply.state;
        }
        self.total = reply.total;
        self.records = reply.records;
        let held = Arc::make_mut(&mut self.held);
        for (i, text) in reply.lines.iter().enumerate() {
            held.insert(
                reply.first + i,
                Held {
                    t: text.clone(),
                    record: None,
                },
            );
        }
        if !reply.lines.is_empty() {
            if let Some(entry) = held.get_mut(&reply.first) {
                entry.record = Some(reply.first_record);
            }
        }
        self.counters.replies += 1;
        self.parsing = true;
        true
    }

    /// The caller's own text with no engine: every line is held at once and drawn plain.
    pub fn answer_plain(&mut self, text: &str) {
        let (held, total) = plain_held(text);
        self.held = Arc::new(held);
        self.total = total;
        self.plain = true;
        self.parsing = true;
    }

    /// The parse the held lines want now.
    pub fn job(&mut self) -> Job {
        self.counters.parses += 1;
        self.counters.lines_parsed += self.held.len();
        Job {
            held: Arc::clone(&self.held),
            total: self.total,
            delim: self.delim,
        }
    }

    /// Where the columns draw: Quick Look's grid, or the boxes that fit the column's frame of `width`.
    pub fn layout(&self, width: f64) -> Vec<Slot> {
        let g = &self.geometry;
        if g.look {
            csv::grid(&self.widths, &self.rights, g.number_width, g.pad_x)
        } else {
            csv::fit(
                &self.widths,
                &self.heads,
                &self.rights,
                g.gap,
                width - 2. * g.edge,
            )
        }
    }

    /// The drawn grid's width: Quick Look's is at least the frame and as wide as its boxes; a column keeps the frame's.
    pub fn grid_width(&self, frame: f64) -> f64 {
        if !self.geometry.look {
            return frame;
        }
        let end = self.layout(frame).last().map_or(0., |slot| slot.x + slot.w);
        frame.max(end)
    }

    /// The text box a slot draws its cell in, from the slot's left edge: Quick Look pads inside, the column insets.
    pub fn cell_box(&self, slot: &Slot) -> (f64, f64) {
        let g = &self.geometry;
        if g.look {
            (slot.x + g.pad_x, (slot.w - 2. * g.pad_x).max(0.))
        } else {
            (g.edge + slot.x, slot.w)
        }
    }
}

/// A cell cut to `cells` character cells, ending in an ellipsis when it does not fit, as a right elide draws it.
pub fn elide(text: &str, cells: usize) -> (String, bool) {
    let count = text.chars().count();
    if count <= cells {
        return (text.to_string(), false);
    }
    if cells == 0 {
        return (String::new(), true);
    }
    let mut out: String = text.chars().take(cells - 1).collect();
    out.push('\u{2026}');
    (out, true)
}

#[path = "csv_measure.rs"]
mod measure;
pub use measure::WIDEN_CELLS;

#[cfg(test)]
#[path = "csv_feed_engine.rs"]
mod engine;

#[cfg(test)]
#[path = "csv_feed_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "csv_feed_records_tests.rs"]
mod records_tests;

#[cfg(test)]
#[path = "csv_feed_parse_tests.rs"]
mod parse_tests;

#[cfg(test)]
#[path = "csv_feed_r3_tests.rs"]
mod r3_tests;
