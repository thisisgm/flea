//! Settings and the keyboard sheet: the panel's state and drawing over the core's rows, edits, search and sheet ranking.
#[cfg(test)]
mod board_strings;
#[cfg(test)]
mod board_tests;
mod draw;
pub mod geometry;
#[cfg(test)]
mod geometry_tests;
mod glyphs;
mod marks;
#[cfg(test)]
mod overlay_tests;
pub mod panel;
pub mod seams;
pub mod sheet;
#[cfg(test)]
mod sheet_board_tests;
#[cfg(test)]
mod sheet_tests;
mod sheet_view;
#[cfg(test)]
mod tests;
pub mod view;
#[cfg(test)]
mod view_tests;

pub use panel::{Panel, Press, Side};
pub use seams::{Facts, Ink, MemoryStore, Request, Store};
pub use sheet::{Sheet, Sources};
pub use sheet_view::SheetView;
pub use view::SettingsView;
