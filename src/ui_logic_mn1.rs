//! Menu bodies shared by the GUI and the TUI: the one inventory, row eligibility, flyouts, fit and scroll rules.
#[cfg(test)]
mod board_tests;
pub mod archive;
pub mod build;
#[cfg(test)]
mod build_tests;
pub mod copyas;
pub mod entry;
pub mod flyouts;
#[cfg(test)]
mod flyouts_tests;
pub mod fit;
#[cfg(test)]
mod fit_tests;
#[cfg(test)]
mod fit_fold_tests;
pub mod inventory;
pub mod places;
#[cfg(test)]
mod places_tests;
pub mod refresh;
pub mod rows;
#[cfg(test)]
mod rows_tests;
#[cfg(test)]
pub(crate) mod testkit;

pub use build::{Context, Kind};
pub use entry::{Brand, Entry, Text};
