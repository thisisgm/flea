//! The order type producers hand the window to dispatch.
use super::{ActionEvent, Chosen};

/// What a producer hands the window to run.
pub(crate) enum Order {
    Action(ActionEvent),
    Menu(Chosen),
    Place(crate::g2::rail_places::producer::Request),
    Device(crate::g2::device_rail::source::Request),
    /// A key the network menu or share browser resolved while one of them is open.
    Network(ActionEvent),
}
