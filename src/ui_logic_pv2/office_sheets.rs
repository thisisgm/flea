//! The spreadsheet bar's sheet steps and words, and the facts row's document count.
use super::{Group, Office, State};

impl Office {
    /// `[` and `]`: the sheet `delta` away, wrapping at the ends as the 3MF plate steps wrap; the new sheet's CSV when it changed.
    pub fn turn_sheet(&mut self, delta: i64) -> Option<String> {
        let count = self.sheet_paths.len() as i64;
        if count == 0 {
            return None;
        }
        let next = (self.sheet as i64 + delta).rem_euclid(count) as usize;
        if next == self.sheet {
            return None;
        }
        self.sheet = next;
        Some(self.sheet_paths[next].clone())
    }

    /// The bar's sheet words in order, the current one marked.
    pub fn sheet_words(&self) -> Vec<(String, bool)> {
        self.sheet_names
            .iter()
            .enumerate()
            .map(|(i, name)| (name.clone(), i == self.sheet))
            .collect()
    }

    /// The facts row that counts the document, Pages, Sheets or Slides, once the count is known.
    pub fn count(&self, pages: usize) -> Option<(&'static str, usize)> {
        let group = self.group?;
        let count = if group == Group::Spreadsheet {
            self.sheet_names.len()
        } else {
            pages
        };
        (self.state == State::Ready && count > 0).then_some((group.count_key(), count))
    }
}
