//! The menu primitive every Flea menu opens through: file, background, header, rail, tags, Copy As and Paste As.
mod delivery;
mod discovery;
mod facts;
pub(crate) mod tags;
mod tags_wire;
mod tag_checks;
pub mod element;
#[cfg(test)]
mod elide_tests;
pub mod glyphs;
pub(crate) mod host_input;
pub mod ipc;
pub mod join;
mod keys;
mod place_host;
#[cfg(test)]
mod join_tests;
pub mod lines;
pub mod menu;
mod owners;
pub(crate) mod presentation;
#[cfg(test)]
mod review_r2_tests;
#[cfg(test)]
mod review_tests;
pub mod shebang;
#[cfg(test)]
mod tests;
mod wrap;

#[cfg(test)]
mod join_delivery_tests;
#[cfg(test)]
mod join_pointer_tests;

#[cfg(test)]
mod join_atomic_tests;

#[cfg(test)]
mod join_fix1_tests;

#[cfg(test)]
mod fix2_prior_proofs;
#[cfg(test)]
mod join_fix2_layout_tests;
#[cfg(test)]
mod join_fix2_tests;

#[cfg(test)]
mod review_wrap_tests;
#[cfg(test)]
mod wrap_layout_tests;
