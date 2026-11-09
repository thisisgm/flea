//! Plain-data stand-ins for contracts other units own, wired at the join.

use flea::ui_logic_device_rail::RailEntry;

/// G1.11 rail host: what the unlock host and the release key read of the rail.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RailView {
    pub places: usize,
    pub network: usize,
    pub devices: Vec<RailEntry>,
    pub cursor: Option<usize>,
}

impl RailView {
    /// The entries in rail order the release key resolves against: places, network, then devices.
    pub fn device_at(&self, index: usize) -> Option<&RailEntry> {
        self.devices
            .get(index.checked_sub(self.places + self.network)?)
    }
}
