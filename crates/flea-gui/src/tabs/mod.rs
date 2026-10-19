//! Tabs: one pane and one backend serve the whole strip, hidden tabs are snapshots, and a tab can move between windows.
pub mod bar;
pub mod state;
pub mod transfer;

pub use bar::{Bar, Panes};
pub use state::{Live, Tab, TabState, Tabs, MAX};
