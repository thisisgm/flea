//! The Trash browser's pure logic, shared by every front end: dates, words, rows, thumbnails, the view and its host.
pub mod dates;
pub mod dropping;
pub mod host;
pub mod monitor;
pub mod rows;
pub mod thumbs;
pub mod selection;
pub mod view;
mod view_ops;
mod view_reply;
pub mod words;

#[cfg(test)]
mod dates_tests;
#[cfg(test)]
mod dropping_tests;
#[cfg(test)]
mod host_tests;
#[cfg(test)]
mod parity_tests;
#[cfg(test)]
mod recovery_tests;
#[cfg(test)]
mod thumbs_tests;
#[cfg(test)]
mod view_tests;
#[cfg(test)]
mod words_tests;
