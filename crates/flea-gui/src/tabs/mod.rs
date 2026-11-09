//! Tabs: one pane and one backend serve the whole strip, hidden tabs are snapshots, and a tab can move between windows.
pub mod bar;
pub mod gone;
pub mod state;
pub mod transfer;

pub use bar::{Bar, Panes};
pub use state::{Live, Tab, TabState, Tabs, MAX};

#[cfg(test)]
mod ack_boundary_tests;
#[cfg(test)]
mod regression_tests;
#[cfg(test)]
pub mod test_support;

mod accessibility;
#[cfg(test)]
mod accessibility_tests;
mod atomic;
pub(crate) mod catcher;
mod geometry;
pub(crate) mod held;
pub(crate) mod host;
#[cfg(test)]
mod join_tests;
mod landing;
mod live;
mod native;
mod overlays;
pub(crate) mod trace;
mod view;

#[cfg(test)]
mod mounted_tests;
#[cfg(test)]
mod router_tests;
#[cfg(test)]
mod s2_tests;

mod middle;

mod context;
