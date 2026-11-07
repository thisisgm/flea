//! The folder sizes a listing asked for, held by row and carried by name across a re-sort.
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
}

impl DirSizes {
    // Only visible rows, only rows the backend called directories, only rows never asked.
    pub fn plan(&self, rows: &[RowFacts], held: usize, first: usize, last: usize) -> Vec<usize> {
        (first..=last)
            .filter(|index| !self.file.contains_key(index))
            .filter(|index| {
                index
                    .checked_sub(held)
                    .and_then(|at| rows.get(at))
                    .is_some_and(|row| row.dir)
            })
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

    // A scroll cancels every row still waiting, matching the backend's own dirsizecancel.
    pub fn cancelled(&mut self) {
        self.file.retain(|_, value| value.is_some());
        let file = &self.file;
        self.order.retain(|row| file.contains_key(row));
    }

    // A re-list carries each answered size to its row's new index by name; pending asks do not follow.
    pub fn carried(
        &self,
        old_rows: &[RowFacts],
        old_held: usize,
        new_rows: &[RowFacts],
        new_held: usize,
    ) -> Self {
        let at: HashMap<&str, usize> = new_rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.dir)
            .map(|(index, row)| (row.name.as_str(), new_held + index))
            .collect();
        let mut next = Self::default();
        for row in &self.order {
            let (Some(Some(answer)), Some(old)) = (
                self.file.get(row),
                row.checked_sub(old_held).and_then(|i| old_rows.get(i)),
            ) else {
                continue;
            };
            if let Some(&index) = at.get(old.name.as_str()) {
                if next.file.insert(index, Some(*answer)).is_none() {
                    next.order.push_back(index);
                }
            }
        }
        next
    }

    // A live answer's own walked sizes land on their absolute rows, over any carried size for the same row.
    pub fn inlined(&mut self, rows: &[RowFacts], start: usize, cap: usize) {
        for (at, row) in rows.iter().enumerate() {
            if let Some(bytes) = row.inline {
                self.remember(
                    start + at,
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
        if self.file.insert(row, Some(size)).is_none() {
            self.order.push_back(row);
        }
        while self.order.len() > cap {
            if let Some(old) = self.order.pop_front() {
                self.file.remove(&old);
            }
        }
    }

    // Both never asked and still waiting draw as the middle dot.
    pub fn size_for(&self, row: usize) -> Option<Size> {
        self.file.get(&row).copied().flatten()
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
