//! The menu primitive every Flea menu opens through: file, background, header, rail, tags, Copy As and Paste As.
#[cfg(test)]
mod app_icon_tests;
pub(crate) mod app_icons;
mod delivery;
mod discovery;
pub mod element;
#[cfg(test)]
mod elide_tests;
mod facts;
#[cfg(test)]
mod fixmenus_tests;
#[cfg(test)]
#[path = "makeexec_foldhash.rs"]
pub(crate) mod foldhash;
pub mod glyphs;
#[cfg(test)]
mod grow_tests;
mod header;
pub(crate) mod host_input;
pub mod ipc;
pub mod join;
#[cfg(test)]
mod join_tests;
pub mod keycaps;
mod keys;
pub mod lines;
pub mod menu;
#[cfg(test)]
mod menu_settle_tests;
#[cfg(test)]
mod menured_tests;
mod notice;
mod owners;
mod place_host;
#[cfg(test)]
mod plugin_mark_tests;
pub(crate) mod presentation;
#[cfg(test)]
mod rclone_tests;
#[cfg(test)]
mod relative_tests;
#[cfg(test)]
mod review_r2_tests;
#[cfg(test)]
mod review_tests;
pub mod shebang;
#[cfg(test)]
mod surface_tests;
mod tag_checks;
pub(crate) mod tags;
mod tags_wire;
pub(crate) mod target;
mod target_action;
#[cfg(test)]
mod target_tests;
#[cfg(test)]
mod tests;
mod wrap;
mod zip;

#[cfg(test)]
mod close_fade_tests;
#[cfg(test)]
mod join_delivery_tests;
#[cfg(test)]
mod join_pointer_tests;
#[cfg(test)]
mod s2_tests;

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
mod ey339_writable_tests;
#[cfg(test)]
mod menu_order2_tests;
#[cfg(test)]
mod review_wrap_tests;
#[cfg(test)]
mod s6_tests;
#[cfg(test)]
mod wrap_layout_tests;
