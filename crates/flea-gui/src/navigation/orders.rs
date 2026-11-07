use super::folder_state::Order;
use std::collections::VecDeque;

#[derive(Default)]
pub(super) struct Orders {
    pub path: String,
    pub preferences: Option<(Order, bool)>,
    pub drawn: Option<(usize, Order)>,
    pub requests: VecDeque<RequestedOrder>,
    // An explicit choice belongs to this open folder; shared memory is for subsequent opens.
    pub explicit: bool,
    // An explicit-path listing owns no folder whose remembered order could be changed.
    pub paths: bool,
}

pub(super) struct RequestedOrder {
    pub order: Order,
    pub sort: bool,
    // A user's sort, which the window's fallback takes when it lands; a tab restoring its own order is not one.
    pub chosen: bool,
}

impl super::Navigator {
    pub(crate) fn is_folder(&self, pane: &crate::pane::Pane) -> bool {
        !pane.state.search && !self.orders.paths
    }

    /// Only leaving Recent ends its explicit paths, so a re-read under Recent cannot claim a folder.
    pub(crate) fn leave_paths(&mut self) {
        self.orders.paths = false;
    }

    /// A sorted replacement can keep select-all made after the sort cleared the old row marks.
    pub(crate) fn sort_landing(&self, pane: &crate::pane::Pane) -> bool {
        self.orders.path == pane.path()
            && self
                .orders
                .requests
                .front()
                .is_some_and(|request| request.sort)
    }
}
