//! Measures a parsed table's columns once the first screen is whole, as the view reaches them: each column's natural
//! width and alignment, taken once, over the first screen's rows for a column the freeze held.
use super::csv;
use super::{body_face, Answer, Counters, Face, Feed};

/// The characters of a long cell measured; the rest of its width is scaled from them by character count.
const PREFIX: usize = 256;

impl Feed {
    /// A parsed table lands; no column is measured until a view reaches it.
    pub fn apply(&mut self, table: csv::Table) {
        self.parsing = false;
        self.table = table;
        if self.plain {
            self.records = self.table.rows.len() + usize::from(self.table.header.is_some());
            self.answer = Answer::Plain;
            self.first_to = self.needed().1;
        }
        if self.table.header.is_none() {
            return;
        }
        let last = self.first_last();
        if !self.frozen && self.covered(1, last) && csv::ready(&self.table, last) {
            self.frozen = true;
            self.frozen_cols = self.table.cols;
        }
    }

    /// Whether `measure` would take a column for a view whose boxes reach `limit`: only once the first screen froze.
    pub fn wants_measure(&self, limit: f64) -> bool {
        self.frozen
            && self.table.header.is_some()
            && self.widths.len() < self.table.cols
            && self.reach <= limit
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

    /// Measures columns left to right while the reach stays within `limit`, each column once: a column the freeze held
    /// takes the first screen's rows, and only a column first appearing after it takes the held rows.
    pub fn measure(&mut self, limit: f64, measure: &mut dyn FnMut(&str, Face) -> f64) {
        if !self.wants_measure(limit) {
            return;
        }
        let mut at = self.reach;
        let Feed {
            table,
            first_to,
            frozen_cols,
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
        while widths.len() < table.cols && at <= limit {
            let c = widths.len();
            // Held first-screen rows are never dropped, so they stay available however late the frame reaches a column.
            let first_screen = c < *frozen_cols;
            let scoped = |row: &&csv::Row| !first_screen || row.record <= *first_to;
            let head = header.get(c).map_or("", String::as_str);
            let head_width = cell_width(head, Face::Head, measure, counters).ceil();
            let mut natural = head_width;
            for row in table.rows.iter().filter(&scoped) {
                let cell = row.cells.get(c).map_or("", String::as_str);
                natural = natural.max(cell_width(cell, body_face(cell), measure, counters));
            }
            let width = natural.ceil();
            let right = csv::numeric_column(
                table
                    .rows
                    .iter()
                    .filter(&scoped)
                    .map(|row| row.cells.get(c).map_or("", String::as_str)),
            );
            heads.push(head_width);
            widths.push(width);
            rights.push(right);
            at += width + span;
        }
        self.reach = at;
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
