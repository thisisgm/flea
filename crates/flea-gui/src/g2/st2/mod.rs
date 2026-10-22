//! Settings, Plugins: the installed list, each plugin's page, Add, Update, Remove and the failed state, over the backend's plugins request.
mod act;
mod card;
pub mod glyphs;
pub mod mark;
pub mod page;
pub mod seam;
pub mod view;

#[cfg(test)]
mod page_tests;
#[cfg(test)]
mod view_tests;
