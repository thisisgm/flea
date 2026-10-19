//! Confirmation, transfers and Jobs: one host, one dialog shell, the cards and the Jobs strip.
mod cards;
pub mod glyph;
pub mod host;
pub mod ipc;
pub mod model;
pub mod palette;
pub mod press;
pub mod view;

#[cfg(test)]
mod draw_tests;
#[cfg(test)]
mod fix1_tests;
#[cfg(test)]
mod fix2_tests;
#[cfg(test)]
mod fix3_tests;
#[cfg(test)]
mod fix4_tests;
#[cfg(test)]
mod fix5_tests;
#[cfg(test)]
mod fix6_tests;
#[cfg(test)]
mod tests;
