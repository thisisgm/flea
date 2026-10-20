//! Settings and the keyboard sheet: the panel's state and drawing over the core's rows, edits, search and sheet ranking.
mod draw;
pub mod geometry;
mod glyphs;
mod marks;
pub mod panel;
pub mod seams;
pub mod sheet;
mod sheet_view;
pub mod view;
#[cfg(test)]
mod board_strings;
#[cfg(test)]
mod board_tests;
#[cfg(test)]
mod geometry_tests;
#[cfg(test)]
mod sheet_board_tests;
#[cfg(test)]
mod sheet_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod view_tests;
#[cfg(test)]
mod overlay_tests;

pub use panel::{Panel, Press, Side};
pub use seams::{Facts, Ink, MemoryStore, Request, Store};
pub use sheet::{Sheet, Sources};
pub use sheet_view::SheetView;
pub use view::SettingsView;
