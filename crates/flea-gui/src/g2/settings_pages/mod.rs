//! Settings and the keyboard sheet: the panel's state and drawing over the core's rows, edits, search and sheet ranking.
#[cfg(test)]
mod about_logo_gap64_tests;
#[cfg(test)]
mod board_strings;
#[cfg(test)]
mod board_tests;
#[cfg(test)]
mod card_height_tests;
pub mod claim;
pub mod cover;
mod draw;
pub mod geometry;
#[cfg(test)]
mod geometry_tests;
pub(crate) mod glyphs;
pub(crate) mod hint_bidi;
#[cfg(test)]
mod ime_tests;
pub(crate) mod input;
mod input_host;
#[cfg(test)]
mod key_preview_atomic_tests;
#[cfg(test)]
mod key_preview_height_tests;
#[cfg(test)]
mod keymap_sheet_activation_tests;
#[cfg(test)]
mod keymap_sheet_menu_tests;
#[cfg(test)]
mod keymap_sheet_order_tests;
#[cfg(test)]
mod label_fit_tests;
#[cfg(test)]
mod label_wrap_tests;
mod marks;
#[cfg(test)]
mod overlay_tests;
pub mod panel;
mod plugins;
pub mod probe;
#[cfg(test)]
mod rail_fit_tests;
#[cfg(test)]
mod room_tests;
#[cfg(test)]
mod scroll_mounted_tests;
#[cfg(test)]
mod scroll_tests;
pub mod seams;
#[cfg(test)]
mod session_tests;
pub mod sheet;
#[cfg(test)]
mod sheet_board_tests;
#[cfg(test)]
mod sheet_keycap_tests;
mod sheet_menus;
#[cfg(test)]
mod column_tests;
#[cfg(test)]
mod sheet_scroll_tests;
#[cfg(test)]
mod sheet_symbol_tests;
#[cfg(test)]
mod sheet_tests;
mod sheet_view;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tiny_window_tests;
pub mod view;
mod view_head;
#[cfg(test)]
mod view_tests;
pub(crate) mod wheel;
#[cfg(test)]
mod word_click_tests;
mod wrap;

pub use panel::{Panel, Press, Side};
pub use seams::{Facts, Ink, MemoryStore, Request, Store};
pub use sheet::{Sheet, Sources};
pub use sheet_view::SheetView;
pub use view::SettingsView;
