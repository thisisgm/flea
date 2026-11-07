//! Menu bodies shared by the GUI and the TUI: the one inventory, row eligibility, flyouts, fit and scroll rules.
pub mod archive;
#[cfg(test)]
mod board_tests;
pub mod build;
#[cfg(test)]
mod build_catalogue_tests;
#[cfg(test)]
mod build_tests;
#[cfg(test)]
mod convert_gate_tests;
pub mod copyas;
#[cfg(test)]
mod copyas_tests;
pub mod entry;
#[cfg(test)]
mod entry_hint_tests;
pub mod fit;
#[cfg(test)]
mod fit_fold_tests;
#[cfg(test)]
mod fit_tests;
#[cfg(test)]
mod fixmenus_tests;
pub mod flyouts;
#[cfg(test)]
mod flyouts_tests;
pub mod inventory;
pub mod places;
#[cfg(test)]
mod places_locked_tests;
#[cfg(test)]
mod places_tests;
#[cfg(test)]
mod rclone_tests;
pub mod refresh;
pub mod rows;
#[cfg(test)]
mod rows_tests;
#[cfg(test)]
mod s2_tests;
#[cfg(test)]
pub(crate) mod testkit;

pub use build::{Context, Kind};
pub use entry::{Brand, Entry, Text};
