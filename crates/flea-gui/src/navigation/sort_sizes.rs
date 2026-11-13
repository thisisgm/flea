//! Folder walks by row, with provisional display answers carried across an in-place re-read.
use crate::list::pane::row_at as facts;
use flea::protocol::RowFacts;
use std::collections::{HashMap, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    pub bytes: u64,
    pub partial: bool,
}

// A row this listing asked about: None is asked and waiting, Some is the answer.
#[derive(Default, Debug)]
pub struct DirSizes {
    file: HashMap<usize, Option<Size>>,
    order: VecDeque<usize>,
    previous: HashMap<usize, Option<Size>>,
}

impl DirSizes {
    // Only visible rows, only rows the backend called directories, only rows never asked.
    pub fn plan(&self, rows: &[RowFacts], held: usize, first: usize, last: usize) -> Vec<usize> {
        (first..=last)
            .filter(|index| !self.file.contains_key(index))
            .filter(|index| facts(rows, held, *index).is_some_and(|row| row.dir))
            .collect()
    }

    pub fn applied(&mut self, ask: &[usize]) {
        for &row in ask {
            if self.file.insert(row, None).is_none() {
                self.order.push_back(row);
            }
        }
    }

    pub fn has_pending(&self) -> bool {
        self.file.values().any(Option::is_none)
    }

    // A full cancellation forgets pending rows, retaining the completed facts.
    pub fn cancelled(&mut self) {
        self.clear_previous();
        self.pause();
    }

    // A replacement listing still owes fresh walks, but retains provisional display bytes.
    pub(crate) fn pause(&mut self) {
        self.file.retain(|_, value| value.is_some());
        let file = &self.file;
        self.order.retain(|row| file.contains_key(row));
    }

    // Selective cancellation forgets only pending rows that left the painted demand.
    pub fn cancel_rows(&mut self, rows: &[usize]) -> bool {
        let before = self.previous.len();
        for row in rows {
            self.previous.remove(row);
        }
        self.file
            .retain(|row, value| value.is_some() || !rows.contains(row));
        let file = &self.file;
        self.order.retain(|row| file.contains_key(row));
        before != self.previous.len()
    }

    // Source identity follows a rename; backends without identity can only match the name.
    pub fn carried(
        &self,
        old_rows: &[RowFacts],
        old_held: usize,
        new_rows: &[RowFacts],
        new_held: usize,
    ) -> Self {
        // Only completed walks carry here; provisional display bytes stay with relisted.
        let answered = Self {
            file: self.file.clone(),
            order: self.order.clone(),
            previous: HashMap::new(),
        };
        answered.carry(old_rows, old_held, new_rows, new_held, usize::MAX)
    }

    fn carry(
        &self,
        old_rows: &[RowFacts],
        old_held: usize,
        new_rows: &[RowFacts],
        new_held: usize,
        cap: usize,
    ) -> Self {
        fn identity(row: &RowFacts) -> (bool, &str) {
            row.source
                .as_deref()
                .map_or((false, row.name.as_str()), |source| (true, source))
        }
        let at: HashMap<_, usize> = new_rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.dir)
            .map(|(index, row)| (identity(row), row.listing_index.unwrap_or(new_held + index)))
            .collect();
        let mut next = Self::default();
        for row in self.order.iter().chain(self.previous.keys()) {
            let (Some(answer), Some(old)) = (self.size_for(*row), facts(old_rows, old_held, *row))
            else {
                continue;
            };
            if let Some(&index) = at.get(&identity(old)) {
                next.remember(index, answer, cap);
            }
        }
        next
    }

    // Display bytes survive, but no answer from the prior listing can satisfy the new planner.
    pub fn relisted(
        &self,
        old_rows: &[RowFacts],
        old_held: usize,
        new_rows: &[RowFacts],
        new_held: usize,
        cap: usize,
    ) -> Self {
        let mut next = self.carry(old_rows, old_held, new_rows, new_held, cap);
        next.previous = std::mem::take(&mut next.file);
        next.order.clear();
        next
    }

    pub fn clear_previous(&mut self) -> bool {
        let changed = !self.previous.is_empty();
        self.previous.clear();
        changed
    }

    // Only the painted demand needs provisional bytes while its replacement walks run.
    pub fn retain_previous(&mut self, rows: &[usize]) -> bool {
        let before = self.previous.len();
        self.previous.retain(|row, _| rows.contains(row));
        before != self.previous.len()
    }

    // A live answer's own walked sizes land on their absolute rows, over any carried size for the same row.
    pub fn inlined(&mut self, rows: &[RowFacts], start: usize, cap: usize) {
        for (at, row) in rows.iter().enumerate() {
            if let Some(bytes) = row.inline {
                self.remember(
                    row.listing_index.unwrap_or(start + at),
                    Size {
                        bytes,
                        partial: false,
                    },
                    cap,
                );
            }
        }
    }

    pub fn remember(&mut self, row: usize, size: Size, cap: usize) {
        self.previous.remove(&row);
        if self.file.insert(row, Some(size)).is_none() {
            self.order.push_back(row);
        }
        while self.order.len() > cap {
            if let Some(old) = self.order.pop_front() {
                self.file.remove(&old);
            }
        }
    }

    // A re-read's pending row keeps its prior display until answered or cancelled.
    pub fn size_for(&self, row: usize) -> Option<Size> {
        self.file
            .get(&row)
            .copied()
            .flatten()
            .or_else(|| self.previous.get(&row).copied().flatten())
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }
}

pub fn wants_sizes(view: &str, hidden_columns: &[&str], class: &str, known: bool) -> bool {
    known
        && !matches!(class, "network" | "phone")
        && view != "grid"
        && !(view == "list" && hidden_columns.contains(&"size"))
}

#[path = "sort_json.rs"]
mod json;

#[cfg(test)]
#[path = "sort_relist_tests.rs"]
mod relist_tests;

#[cfg(test)]
mod sparse_tests {
    use super::*;

    #[test]
    fn fx477_sparse_folder_sizes_plan_inline_and_carry_by_listing_index() {
        let rows = [
            RowFacts {
                name: "a".into(),
                dir: true,
                listing_index: Some(0),
                ..Default::default()
            },
            RowFacts {
                name: "b".into(),
                dir: true,
                listing_index: Some(60),
                inline: Some(243),
                ..Default::default()
            },
        ];
        let mut sizes = DirSizes::default();
        assert_eq!(sizes.plan(&rows, 0, 60, 60), [60]);
        assert!(sizes.plan(&rows, 0, 1, 1).is_empty());
        sizes.inlined(&rows, 0, 240);
        assert_eq!(sizes.size_for(60).unwrap().bytes, 243);
        assert!(sizes.size_for(1).is_none());
        let moved = [
            RowFacts {
                name: "x".into(),
                dir: true,
                listing_index: Some(100),
                ..Default::default()
            },
            RowFacts {
                name: "b".into(),
                dir: true,
                listing_index: Some(120),
                ..Default::default()
            },
        ];
        let carried = sizes.carried(&rows, 0, &moved, 100);
        assert_eq!(carried.size_for(120).unwrap().bytes, 243);
        assert!(carried.size_for(60).is_none());
        assert!(carried.size_for(101).is_none());
    }
}
