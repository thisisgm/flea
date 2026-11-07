//! Network shares and the Add a Network Share surface.
pub mod bookmarks;
pub mod browser;
mod buttons;
mod cancel;
mod child;
pub mod dialog;
mod drawing;
mod geometry;
mod marks;
pub mod menus;
pub mod mount;
pub mod nearby;
mod pointer;
mod present;
pub mod provider;
mod provider_view;
pub mod relay;
pub mod result;
pub mod routing;
pub mod service;
mod status;
mod validate;
pub mod view;
mod words;

#[cfg(test)]
mod bfx_net_tests;
#[cfg(test)]
mod bridge_line_tests;
#[cfg(test)]
mod child_tests;
#[cfg(test)]
mod dialog_tests;
#[cfg(test)]
mod extra_tests;
#[cfg(test)]
mod guest_tests;
#[cfg(test)]
mod input_tests;
#[cfg(test)]
mod locale_tests;
#[cfg(test)]
mod provider_tests;
#[cfg(test)]
mod runtime_tests;
#[cfg(test)]
mod store_tests;
#[cfg(test)]
mod tests;

mod input;
mod keys;
mod layout;
mod secret_edit;

mod mount_bridge;

mod mount_process;

mod mount_lifetime;

mod session;
#[cfg(test)]
#[path = "session_tests.rs"]
mod session_tests;

mod view_observe;
mod view_render;

#[cfg(test)]
mod fix1_flow_tests;
#[cfg(test)]
mod fix1_tests;
#[cfg(test)]
mod fix2_tests;
#[cfg(test)]
mod m9_g2rows_19_tests;
#[cfg(test)]
mod review_tests;

#[cfg(test)]
mod i50_dialog_tests;
