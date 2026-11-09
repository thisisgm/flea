//! Header and s/S sort decisions, and the folder sizes that follow their rows by name.
use super::folder_state::{Order, ORDERS};

#[path = "sort_sizes.rs"]
mod sizes;
pub use sizes::{wants_sizes, DirSizes, Size};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    Order(Order),
    Restore(Order),
    Forget,
}

// A tab handoff carries its live order, independently of the shared folder preference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub path: String,
    pub order: Order,
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

// A submitted folder choice already drives a sort or re-list; acknowledge it before its disk echo.
pub(crate) fn apply_own_patch(data: &mut flea::jsondoc::Json, command: &super::Command) {
    let super::Command::Persist(text) = command else {
        return;
    };
    if let Some(next) = flea::jsondoc::parse(text)
        .ok()
        .filter(|patch| patch.get("folderSorts").is_some())
        .and_then(|patch| flea::uistate::patched(data, &patch).ok())
    {
        *data = next;
    }
}

impl super::Navigator {
    pub(crate) fn fresh_preferences(&mut self, pane: &mut crate::pane::Pane) -> bool {
        self.orders.preferences = None;
        self.fresh(pane).unwrap_or(false)
    }

    // Shared settings refresh the listing options while an explicit order stays with the open folder.
    pub(crate) fn reread_preferences(
        &mut self,
        pane: &mut crate::pane::Pane,
        selected: &[usize],
    ) -> bool {
        self.orders.preferences = None;
        self.reread(pane, selected, true, false)
    }

    // A pending listing or sort has no settled path/order pair to hand to another window.
    pub fn sort_snapshot(&self, pane: &crate::pane::Pane) -> Result<Snapshot, super::Refusal> {
        if pane.busy()
            || !pane.swap.sorts_out.is_empty()
            || pane.swap.unheld_total.is_some()
            || !self.orders.requests.is_empty()
        {
            return Err(super::Refusal::Busy);
        }
        if pane.state.search
            || !matches!(
                pane.state.kind,
                crate::pane::Kind::Ready | crate::pane::Kind::Empty
            )
        {
            return Err(super::Refusal::NotLoaded);
        }
        Ok(Snapshot {
            path: pane.path().into(),
            order: self.order,
        })
    }

    // Restore one tab listing without remembering a choice; ordinary navigation reads folder defaults again.
    pub fn restore_sort_snapshot(
        &mut self,
        pane: &mut crate::pane::Pane,
        snapshot: &Snapshot,
    ) -> Result<bool, super::Refusal> {
        if snapshot.path.is_empty()
            || snapshot.path.contains('\0')
            || !ORDERS.contains(&snapshot.order.key)
        {
            return Err(super::Refusal::NotLoaded);
        }
        let previous = self.order;
        let preserve = std::mem::replace(&mut self.preserve_sort, true);
        self.order = snapshot.order;
        let result = self.open_without_history(
            pane,
            &snapshot.path,
            crate::pane::swap::Options {
                supersede: true,
                ..Default::default()
            },
            None,
            false,
        );
        self.preserve_sort = preserve;
        if result != Ok(true) {
            self.order = previous;
        }
        result
    }

    fn holds_sort(&self) -> bool {
        self.gate.editing || self.gate.rename_pending
    }

    // A user's sort: the folder remembers it, or with memory off the window's fallback takes it once it lands.
    pub fn resort(
        &mut self,
        pane: &mut crate::pane::Pane,
        order: Order,
        selected: &mut Vec<usize>,
        failure: String,
    ) -> Result<bool, super::Refusal> {
        self.reorder(pane, order, selected, failure, true)
    }

    // A tab coming back on the same folder shows its own order without replacing what the folder remembers.
    pub fn restore_order(
        &mut self,
        pane: &mut crate::pane::Pane,
        order: Order,
        selected: &mut Vec<usize>,
    ) -> Result<bool, super::Refusal> {
        self.reorder(pane, order, selected, String::new(), false)
    }

    // Only an order the backend produces moves the mark; the same order is not asked for at all.
    fn reorder(
        &mut self,
        pane: &mut crate::pane::Pane,
        order: Order,
        selected: &mut Vec<usize>,
        failure: String,
        chosen: bool,
    ) -> Result<bool, super::Refusal> {
        if self.holds_sort() {
            self.pending_sort = Some(if chosen {
                Request::Order(order)
            } else {
                Request::Restore(order)
            });
            return Ok(false);
        }
        if pane.busy() {
            return Err(super::Refusal::Busy);
        }
        if order == self.order {
            return Ok(false);
        }
        let folders = self.folders.borrow().clone();
        self.start_sort(pane, order, chosen);
        self.emit(super::Command::Sort {
            order,
            folders_first: folders.folders_first,
            group_by_kind: folders.group_by_kind,
            hidden_last: folders.hidden_last,
        });
        self.order = order;
        if chosen && self.is_folder(pane) {
            let patch = self.folders.borrow_mut().remember(pane.path(), order);
            if let Some(patch) = patch {
                self.persist(patch, failure);
            }
        }
        // Clear row-indexed marks now; keep the retained cursor and viewport until sorted rows land.
        selected.clear();
        // While a pair is armed, a request out at the resorted top answers before this sort's listed line, so the pair takes it at its own count.
        if pane.swap.unheld_total.is_none()
            || pane
                .requests
                .outstanding
                .is_some_and(|request| request.start != 0)
        {
            pane.requests.cancel();
        }
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
        if pane.busy() {
            return Err(super::Refusal::Busy);
        }
        let path = pane.path().to_owned();
        let patch = self.folders.borrow_mut().forget(&path);
        if let Some(patch) = patch {
            self.persist(patch, failure);
        }
        self.orders.preferences = None;
        self.orders.explicit = false;
        let preserve = std::mem::replace(&mut self.preserve_sort, false);
        let result = self.open_without_history(pane, &path, Default::default(), None, false);
        self.preserve_sort = preserve;
        result
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
            Some(Request::Restore(order)) => self.restore_order(pane, order, selected),
            Some(Request::Forget) => self.forget_sort(pane, failure),
            None => Ok(false),
        }
    }
}

#[cfg(test)]
#[path = "sort_inline_tests.rs"]
mod inline_tests;
