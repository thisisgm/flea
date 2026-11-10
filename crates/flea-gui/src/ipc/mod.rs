pub(crate) mod archive_cards;
pub(crate) mod browse_state;
pub(crate) mod card_geometry;
pub mod chrome;
pub mod columns;
pub(crate) mod columns_preview;
pub mod devices;
pub(crate) mod dialog_cards;
pub(crate) mod drag_state;
pub mod dual;
pub(crate) mod favourites;
pub(crate) mod fileops_preview;
pub mod focus;
pub mod grid;
pub(crate) mod grid_rows;
pub(crate) mod keyboard_state;
pub(crate) mod main_cohesion;
mod menu_cards;
pub mod native_window;
pub mod navigation;
pub(crate) mod network;
pub(crate) mod network_mountinfo;
mod open_with_painted;
pub(crate) mod outline;
pub(crate) mod output_cards;
pub mod pane;
pub mod path;
pub(crate) mod preview_html;
pub(crate) mod preview_look;
pub(crate) mod preview_scroll;
pub(crate) mod preview_state;
pub mod rail;
pub(crate) mod rail_devices;
pub(crate) mod recent;
pub mod rows;
pub mod server;
pub(crate) mod settings;
pub(crate) mod status;
mod status_activity;
mod status_card;
pub mod strips;
mod surface;
pub mod tabs;
pub(crate) mod thumbnails;
pub mod view;
pub(crate) mod window_feedback;

#[cfg(test)]
mod preview_scroll_tests;

#[cfg(test)]
mod fixreaders_tests;
#[cfg(test)]
mod reader_port_b_tests;

pub use surface::{Adapter, Surface};
pub(crate) mod preview_text;

#[cfg(test)]
mod pb_ipc_fixture;
#[cfg(test)]
mod pb_ipc_tests;
#[cfg(test)]
mod plugin_input_tests;
#[cfg(test)]
mod reader_port_a_tests;
#[cfg(test)]
mod reader_port_c_tests;
#[cfg(test)]
mod readers3_tests;
#[cfg(test)]
mod readers_nav_tests;
#[cfg(test)]
mod vf3_tests;

#[cfg(test)]
mod vfix_tests;

#[cfg(test)]
mod vfix_table_tests;

#[cfg(test)]
mod pc2_output_geometry_tests;
#[cfg(test)]
mod vfix_card_tests;
