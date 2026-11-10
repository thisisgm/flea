//! Measures a parsed table's columns once the first screen is whole: a reached column over the first screen's rows, then each held cell once, a bounded slice a frame.
use super::csv;
use super::phase::{Columns, Parse};
use super::{body_face, Answer, Counters, Face, Feed};

/// The characters of a long cell measured; the rest of its width is scaled from them by character count.
const PREFIX: usize = 256;
/// Held cells one `measure` call widens by, about a screen of a wide table; later calls resume at the row it stopped on.
pub const WIDEN_CELLS: usize = 512;

impl Feed {
    /// A parsed table lands; no column is measured until a view reaches it, and its new rows may widen the measured.
    pub fn apply(&mut self, table: csv::Table) {
        self.parse_phase = self.parse_phase.finish();
        self.table = table;
        if self.widening() {
            self.column_phase = Columns::Widening(0);
        }
        if self.parse_phase == Parse::Plain {
            self.records = self.table.rows.len() + usize::from(self.table.header.is_some());
            self.answer = Answer::Plain;
            self.first_to = self.needed().1;
        }
        if self.table.header.is_none() {
            return;
        }
        let last = self.first_last();
        if self.frozen() || (self.covered(1, last) && csv::ready(&self.table, last)) {
            self.column_phase = Columns::Widening(0);
        }
    }

    /// The first screen fixed the column alignments; later held cells may still widen them.
    pub fn frozen(&self) -> bool {
        self.column_phase != Columns::Waiting
    }

    /// Held cells remain to be measured by a later frame.
    pub fn widening(&self) -> bool {
        matches!(self.column_phase, Columns::Widening(_))
    }

    /// The next held row to widen, or the end once all held rows were measured.
    pub fn widen_from(&self) -> usize {
        match self.column_phase {
            Columns::Waiting => 0,
            Columns::Widening(from) => from,
            Columns::Measured => self.table.rows.len(),
        }
    }

    /// Whether `measure` has work for a view whose boxes reach `limit`: a column to take, or held cells to widen by.
    pub fn wants_measure(&self, limit: f64) -> bool {
        self.frozen()
            && self.table.header.is_some()
            && (self.widening() || (self.widths.len() < self.table.cols && self.reach <= limit))
    }

    /// The reach a frame needs measured: Quick Look's right edge past the scroll, or the column's fit plus one gap.
    pub fn reach_target(&self, scroll: f64, frame: f64) -> f64 {
        let g = &self.geometry;
        if g.look {
            scroll + frame
        } else {
            frame - 2. * g.edge + g.gap
        }
    }

    /// Takes the columns the reach meets, left to right, then widens the measured ones by at most `WIDEN_CELLS` held cells.
    pub fn measure(&mut self, limit: f64, measure: &mut dyn FnMut(&str, Face) -> f64) {
        if !self.wants_measure(limit) {
            return;
        }
        self.take_columns(limit, measure);
        self.widen(measure);
    }

    // A new column's head, width and alignment from the first screen's rows, which are never dropped once held.
    fn take_columns(&mut self, limit: f64, measure: &mut dyn FnMut(&str, Face) -> f64) {
        let mut at = self.reach;
        let Feed {
            table,
            first_to,
            folded,
            column_phase,
            heads,
            widths,
            rights,
            geometry,
            counters,
            ..
        } = self;
        let Some(header) = table.header.as_deref() else {
            return;
        };
        let span = if geometry.look {
            2. * geometry.pad_x
        } else {
            geometry.gap
        };
        let first_screen = |row: &&csv::Row| row.record <= *first_to;
        while widths.len() < table.cols && at <= limit {
            let c = widths.len();
            let head = header.get(c).map_or("", String::as_str);
            let head_width = cell_width(head, Face::Head, measure, counters).ceil();
            let mut natural = head_width;
            for row in table.rows.iter().filter(&first_screen) {
                let cell = row.cells.get(c).map_or("", String::as_str);
                natural = natural.max(cell_width(cell, body_face(cell), measure, counters));
                let done = folded.entry(row.record).or_insert(0);
                if *done == c {
                    *done = c + 1;
                }
            }
            let width = natural.ceil();
            let right = csv::numeric_column(
                table
                    .rows
                    .iter()
                    .filter(&first_screen)
                    .map(|row| row.cells.get(c).map_or("", String::as_str)),
            );
            heads.push(head_width);
            widths.push(width);
            rights.push(right);
            at += width + span;
            // Every held row now owes this column a cell, so the next widening walks from the first row again.
            *column_phase = Columns::Widening(0);
        }
        self.reach = at;
    }

    // Each held cell a measured column has not taken widens it, once; `widening` stays set while rows are left.
    fn widen(&mut self, measure: &mut dyn FnMut(&str, Face) -> f64) {
        let Feed {
            table,
            folded,
            widths,
            counters,
            reach,
            revision,
            column_phase,
            ..
        } = self;
        let Columns::Widening(widen_from) = column_phase else {
            return;
        };
        let mut left = WIDEN_CELLS;
        let mut grown = 0.;
        while let Some(row) = table.rows.get(*widen_from) {
            let done = folded.entry(row.record).or_insert(0);
            while *done < widths.len() && left > 0 {
                let cell = row.cells.get(*done).map_or("", String::as_str);
                // An empty cell is never wider than a column, so it costs no measure.
                if !cell.is_empty() {
                    let width = cell_width(cell, body_face(cell), measure, counters).ceil();
                    let column = &mut widths[*done];
                    if width > *column {
                        grown += width - *column;
                        *column = width;
                    }
                    left -= 1;
                }
                *done += 1;
            }
            if *done < widths.len() {
                break;
            }
            *widen_from += 1;
        }
        if *widen_from >= table.rows.len() {
            *column_phase = Columns::Measured;
        }
        if grown > 0. {
            *reach += grown;
            *revision += 1;
        }
    }
}

// A cell's width: its whole text for a short cell; a long cell's first PREFIX characters, scaled by its character count.
fn cell_width(
    text: &str,
    face: Face,
    measure: &mut dyn FnMut(&str, Face) -> f64,
    counters: &mut Counters,
) -> f64 {
    counters.measured += 1;
    let count = text.chars().count();
    if count <= PREFIX {
        return measure(text, face);
    }
    let end = text
        .char_indices()
        .nth(PREFIX)
        .map_or(text.len(), |(at, _)| at);
    measure(&text[..end], face) * count as f64 / PREFIX as f64
}
