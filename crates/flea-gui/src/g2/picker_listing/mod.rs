//! Listing/filter/Save controllers; pk1 owns window, G1.06 rows.
pub mod chip_input;
pub mod chrome;
pub mod listing;
pub(crate) mod pictures;
mod pointer;
pub mod route;
pub mod save;
mod scrollbar;
mod settings;
pub mod surface;
pub mod view;

pub use listing::{Cause, Escape, Facts, Filtered, Listing, NameMatcher, SubstringMatcher};
pub use save::{ProbeKey, SaveField, SaveWords};

#[cfg(test)]
mod chrome_tests;
#[cfg(test)]
mod keys_tests;
#[cfg(test)]
mod listing_tests;
#[cfg(test)]
mod rev_r4_tests;
#[cfg(test)]
mod route_ports_tests;
#[cfg(test)]
mod route_tests;
#[cfg(test)]
mod save_tests;
#[cfg(test)]
mod sort_tests;
#[cfg(test)]
mod strip_tests;
#[cfg(test)]
mod view_tests;

/// Recent locations read from the places service; pl4 owns the reader.
pub trait RecentSource {
    fn recent_paths(&self) -> Vec<String>;
}
