//! pl1: devices, phones, eject and power-off, and the encrypted-drive unlock card.
pub mod card;
pub mod devices;
pub mod eject;
pub mod field;
pub mod host;
pub mod ink;
pub mod keys;
pub mod listing;
pub mod phones;
pub mod process;
pub mod seams;
pub mod secret;
pub mod source;
pub mod unlock;
pub mod view;
pub mod words;

#[cfg(test)]
mod card_tests;
#[cfg(test)]
mod eject_tests;
#[cfg(test)]
mod host_tests;
#[cfg(test)]
mod keys_tests;
#[cfg(test)]
mod listing_tests;
#[cfg(test)]
mod process_tests;
#[cfg(test)]
mod unlock_tests;
#[cfg(test)]
mod view_quad_tests;
#[cfg(test)]
mod view_tests;
