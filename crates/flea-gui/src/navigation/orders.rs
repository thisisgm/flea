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
}

pub(super) struct RequestedOrder {
    pub order: Order,
    pub sort: bool,
}

impl super::Navigator {
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
