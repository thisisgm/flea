//! Open With, the run card, Permissions, Properties and bulk rename: card models over core's ui_logic_open_with_properties, drawn through the dialog shell seam.
pub mod bulkrename;
#[cfg(test)]
mod card_props_tests;
#[cfg(test)]
mod card_tests;
#[cfg(test)]
mod fixcards2_tests;
#[cfg(test)]
mod fixcards_tests;
pub(crate) mod initial;
pub mod keys;
pub mod lane;
pub mod openwith;
pub mod permissions;
#[cfg(test)]
mod permissions_tests;
pub mod properties;
#[cfg(test)]
mod review_tests;
pub mod runcard;
pub mod shell;
#[cfg(test)]
pub(crate) mod tests;
#[cfg(test)]
mod window_tests;

/// What a card asks its host to do; the host owns the backend, the processes and the status line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// One request line for the backend.
    Send(String),
    /// The card is done and leaves the shell.
    Close,
    /// An argv the opener lane spawns off the UI thread, never a shell string.
    Launch(Vec<String>),
    /// A status line in the plain role.
    Message(String),
    /// Bulk rename's names for the editor the session names, opened in a terminal over a private scratch file.
    EditNames { folder: String, names: Vec<String> },
    /// The pane rereads its folder and keeps its marks (G1 Step::Refresh).
    Refresh,
}
