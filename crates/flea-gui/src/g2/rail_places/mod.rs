//! pl4: the places, favorites, tags, recents and plugin sections of the rail, as data the G1 rail host draws.
pub mod content;
pub mod menus;
pub mod producer;
#[path = "../../rail/rows.rs"]
pub mod rows;
pub mod seams;
pub mod service;

#[cfg(test)]
mod fix1_tests;
#[cfg(test)]
mod i50_ime_tests;
#[cfg(test)]
mod place_edge_tests;
#[cfg(test)]
mod service_tests;
#[cfg(test)]
mod tests;
