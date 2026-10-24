//! The panel's standing search: its hits, the highlighted result and the keys typed while the field is open.
use super::{Panel, Side};
use flea::ui_logic_st1::{
    pages::SECTIONS,
    rows::{focusable, Row},
};

impl Panel {
    /// A section's rows as the standing search read them.
    pub fn everything_rows(&self, section: usize) -> &[Row] {
        self.everything
            .iter()
            .find(|(index, _)| *index == section)
            .map_or(&[], |(_, rows)| rows)
    }

    pub(super) fn hit_count(&self) -> usize {
        self.results
            .iter()
            .map(|group| group.hits.len().max(1))
            .sum()
    }

    /// The section and row the highlighted result opens at.
    pub fn hit_target(&self) -> Option<(usize, usize)> {
        let mut left = self.hit;
        for group in &self.results {
            let span = group.hits.len().max(1);
            if left < span {
                let row = group.hits.get(left).map(|hit| hit.row).or(group.heading)?;
                return Some((group.section, row));
            }
            left -= span;
        }
        None
    }

    pub fn open_hit(&mut self) {
        let Some((section, row)) = self.hit_target() else {
            return;
        };
        self.query = None;
        self.results.clear();
        self.show_section(SECTIONS[section].id);
        self.side = Side::Pane;
        if self.rows.get(row).is_some_and(focusable) {
            self.cursor = row;
        }
        self.reveal();
    }

    pub(super) fn search_key(&mut self, key: &str, text: &str) {
        let query = self.query.get_or_insert_with(String::new);
        match key {
            "escape" => {
                self.query = None;
                self.results.clear();
                return;
            }
            "backspace" if query.is_empty() => {
                self.query = None;
                return;
            }
            "backspace" => {
                query.pop();
            }
            "down" => self.hit = (self.hit + 1).min(self.hit_count().saturating_sub(1)),
            "up" => self.hit = self.hit.saturating_sub(1),
            "enter" => return self.open_hit(),
            _ if flea::ui_logic_st1::sheetkeys::is_printable(text) => query.push_str(text),
            _ => return,
        }
        if matches!(key, "up" | "down") {
            self.hit_moved = true;
            return;
        }
        self.hit = 0;
        self.results_scroll = 0.;
        self.rebuild();
    }
}
