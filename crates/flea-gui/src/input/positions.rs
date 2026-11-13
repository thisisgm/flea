//! One immutable forward filter map and its inverse, rebuilt only for a new answer.
use super::PaneInput;
use std::rc::Rc;

#[derive(Default)]
pub struct Rows {
    rows: Option<Rc<[usize]>>,
    /// Empty for an ascending answer, which is searched directly.
    inverse: Vec<(usize, usize)>,
}

impl Rows {
    pub(super) fn new(rows: Option<Rc<[usize]>>) -> Self {
        let mut inverse = Vec::new();
        if let Some(rows) = rows.as_ref().filter(|rows| !rows.is_sorted()) {
            inverse = rows
                .iter()
                .enumerate()
                .map(|(view, &row)| (row, view))
                .collect();
            inverse.sort_unstable_by_key(|&(row, _)| row);
        }
        Self { rows, inverse }
    }

    /// A listing row's drawn position; ascending answers keep their original binary search.
    pub fn view_of(&self, row: usize) -> Option<usize> {
        let Some(rows) = &self.rows else {
            return Some(row);
        };
        if self.inverse.is_empty() {
            rows.binary_search(&row).ok()
        } else {
            self.inverse
                .binary_search_by_key(&row, |&(row, _)| row)
                .ok()
                .map(|at| self.inverse[at].1)
        }
    }

    /// Whether the answer is in listing order, so listing ranges and insertion points still hold.
    pub fn ascending(&self) -> bool {
        self.inverse.is_empty()
    }
}

impl std::ops::Deref for Rows {
    type Target = Option<Rc<[usize]>>;

    fn deref(&self) -> &Self::Target {
        &self.rows
    }
}

impl PaneInput {
    pub fn index(&self, view: usize) -> usize {
        self.shown
            .as_ref()
            .and_then(|shown| shown.get(view))
            .copied()
            .unwrap_or(view)
    }

    /// A hidden row stands at its insertion point in an ascending answer and before view 0 in a ranked one.
    pub fn view_index(&self, row: usize) -> usize {
        match self.shown.as_deref() {
            Some(rows) if self.shown.ascending() => {
                rows.binary_search(&row).unwrap_or_else(|at| at)
            }
            _ => self.shown.view_of(row).unwrap_or(0),
        }
    }
}
