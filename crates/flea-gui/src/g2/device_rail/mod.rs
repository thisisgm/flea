//! pl1: devices, phones, eject and power-off, and the encrypted-drive unlock card.
pub mod card;
pub mod card_text;
pub mod devices;
pub mod eject;
pub mod eye;
pub mod field;
pub mod host;
pub mod ink;
pub mod keys;
pub mod listing;
mod lock;
pub mod mark;
pub mod phones;
pub mod probes;
pub mod process;
pub mod seams;
pub mod secret;
pub mod source;
pub mod unlock;
pub mod view;
pub mod words;

#[cfg(test)]
pub(crate) mod bfx_net_tests;
#[cfg(test)]
mod backspace_tests;
#[cfg(test)]
mod bitlocker_tests;
#[cfg(test)]
mod card_tests;
#[cfg(test)]
mod eject_restore_tests;
#[cfg(test)]
mod eject_tests;
#[cfg(test)]
mod host_tests;
#[cfg(test)]
mod keys_tests;
#[cfg(test)]
mod listing_tests;
#[cfg(test)]
mod lock_tests;
#[cfg(test)]
mod process_tests;
#[cfg(test)]
mod unlock_tests;
#[cfg(test)]
mod view_quad_tests;
#[cfg(test)]
mod view_scale_tests;
#[cfg(test)]
mod view_tests;

mod ime;
pub(crate) mod private_entry;

#[cfg(test)]
mod i50_dialog_tests;
