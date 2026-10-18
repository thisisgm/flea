//! Header and s/S sort decisions, and the folder sizes that follow their rows by name.
use super::folder_state::{Order, ORDERS};
use flea::protocol::RowFacts;
use std::collections::{HashMap, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    Order(Order),
    Forget,
}

// The sorted column reverses; any other starts ascending.
pub fn column_order(current: Order, key: &str) -> Option<Order> {
    let key = ORDERS.iter().copied().find(|order| *order == key)?;
    Some(Order {
        key,
        desc: current.key == key && !current.desc,
    })
}

// Always ascending; an outside order starts over at the first.
pub fn next_order(current: Order) -> Order {
    let at = ORDERS.iter().position(|key| *key == current.key);
    Order {
        key: ORDERS[at.map_or(0, |at| (at + 1) % ORDERS.len())],
        desc: false,
    }
}

pub fn reverse_order(current: Order) -> Order {
    Order {
        key: current.key,
        desc: !current.desc,
    }
}

// The geometric mark of the direction, characters the stock monospace has.
pub fn title(label: &str, desc: bool) -> String {
    format!("{label} {}", if desc { '\u{25be}' } else { '\u{25b4}' })
}

pub fn mark(order: Order) -> String {
    format!("{}:{}", order.key, if order.desc { "desc" } else { "asc" })
}

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

impl super::Navigator {
    fn holds_sort(&self) -> bool {
        self.gate.editing || self.gate.rename_pending
    }

    // Only an order the backend produces moves the mark; the same order is not asked for at all.
    pub fn resort(
        &mut self,
        pane: &mut crate::pane::Pane,
        order: Order,
        selected: &mut Vec<usize>,
        failure: String,
    ) -> Result<bool, super::Refusal> {
        if self.holds_sort() {
            self.pending_sort = Some(Request::Order(order));
            return Ok(false);
        }
        if pane.busy() {
            return Err(super::Refusal::Busy);
        }
        if order == self.order {
            return Ok(false);
        }
        let folders = self.folders.borrow().clone();
        self.emit(super::Command::Sort {
            order,
            folders_first: folders.folders_first,
            group_by_kind: folders.group_by_kind,
            hidden_last: folders.hidden_last,
        });
        self.order = order;
        if !pane.state.search {
            let patch = self.folders.borrow_mut().remember(pane.path(), order);
            if let Some(patch) = patch {
                self.persist(patch, failure);
            }
        }
        // A reorder moves every row, so row-indexed marks would silently name different files.
        selected.clear();
        pane.cursor = 0;
        pane.scroll_to(0.);
        pane.requests.cancel();
        // The sorted pair lands unheld, so the pane checks it against the count asked for here.
        let count = pane.first_count();
        pane.swap.sorts_out.push_back(count);
        self.emit(super::Command::Window { start: 0, count });
        Ok(true)
    }

    pub fn sort_column(
        &mut self,
        pane: &mut crate::pane::Pane,
        key: &str,
        selected: &mut Vec<usize>,
        failure: String,
    ) -> Result<bool, super::Refusal> {
        match column_order(self.order, key) {
            Some(order) => self.resort(pane, order, selected, failure),
            None => Ok(false),
        }
    }

    pub fn sort_next(
        &mut self,
        pane: &mut crate::pane::Pane,
        selected: &mut Vec<usize>,
        failure: String,
    ) -> Result<bool, super::Refusal> {
        let order = next_order(self.order);
        self.resort(pane, order, selected, failure)
    }

    pub fn sort_reverse(
        &mut self,
        pane: &mut crate::pane::Pane,
        selected: &mut Vec<usize>,
        failure: String,
    ) -> Result<bool, super::Refusal> {
        let order = reverse_order(self.order);
        self.resort(pane, order, selected, failure)
    }

    // The flyout's last row forgets the folder and lists it on the default order.
    pub fn forget_sort(
        &mut self,
        pane: &mut crate::pane::Pane,
        failure: String,
    ) -> Result<bool, super::Refusal> {
        if self.holds_sort() {
            self.pending_sort = Some(Request::Forget);
            return Ok(false);
        }
        let path = pane.path().to_owned();
        let patch = self.folders.borrow_mut().forget(&path);
        if let Some(patch) = patch {
            self.persist(patch, failure);
        }
        self.open_without_history(pane, &path, Default::default(), None, false)
    }

    // The held sort runs once the edit ends; a listing request in between drops it.
    pub fn apply_pending_sort(
        &mut self,
        pane: &mut crate::pane::Pane,
        selected: &mut Vec<usize>,
        failure: String,
    ) -> Result<bool, super::Refusal> {
        if self.holds_sort() || pane.busy() {
            return Ok(false);
        }
        match self.pending_sort.take() {
            Some(Request::Order(order)) => self.resort(pane, order, selected, failure),
            Some(Request::Forget) => self.forget_sort(pane, failure),
            None => Ok(false),
        }
    }
}
