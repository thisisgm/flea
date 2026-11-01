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
