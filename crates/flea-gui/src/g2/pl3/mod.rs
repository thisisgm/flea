//! pl3, the Trash browser: the core session in flea::ui_logic_pl3 drawn by custom Elements behind narrow seams.
pub mod board;
mod elide;
mod glyph;
pub mod ink;
pub mod list;
pub mod monitor;
pub mod pane;
pub mod seams;
mod strip;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod paint_tests;
#[cfg(test)]
mod flow_tests;
#[cfg(test)]
mod fix_tests;
