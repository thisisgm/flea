//! Application-wide plugin service: one owner process, shared by every window and pane.
mod host;
pub(crate) mod input;
mod join;
pub(crate) mod limits;
mod shelf;
pub(crate) use host::{service, Closed, Host, Response, WireLine};
pub(crate) use join::Join;

pub(crate) mod menu;
mod place;
mod place_model;
mod place_ops;
pub(crate) mod rail;

#[cfg(test)]
mod place_validation_tests;
#[cfg(test)]
mod plugin_place_paste_tests;
#[cfg(test)]
mod rail_place_fixture;
#[cfg(test)]
mod rail_place_tests;
