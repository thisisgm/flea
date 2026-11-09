//! Settings, Plugins: the installed list, each plugin's page, Add, Update, Remove and the failed state, over the backend's plugins request.
mod act;
mod airdrop;
mod card;
pub mod glyphs;
mod input;
mod load;
pub mod mark;
pub mod page;
mod scroll;
pub mod seam;
pub mod view;

#[cfg(test)]
mod airdrop_tests;
#[cfg(test)]
mod fact_tests;
#[cfg(test)]
mod page_tests;
#[cfg(test)]
mod scroll_tests;
#[cfg(test)]
mod view_tests;

#[cfg(test)]
mod ime_tests;
