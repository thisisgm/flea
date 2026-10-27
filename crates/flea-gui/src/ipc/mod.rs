pub(crate) mod archive_cards;
pub mod chrome;
pub mod columns;
pub(crate) mod columns_preview;
pub mod devices;
pub(crate) mod dialog_cards;
pub mod dual;
pub mod focus;
pub mod grid;
pub(crate) mod grid_rows;
pub mod native_window;
pub mod navigation;
pub(crate) mod network;
pub(crate) mod output_cards;
pub mod pane;
pub mod path;
pub(crate) mod preview_scroll;
pub mod rail;
pub mod rows;
pub mod server;
pub(crate) mod settings;
pub(crate) mod status;
mod status_card;
pub mod strips;
mod surface;
pub mod tabs;
pub(crate) mod thumbnails;
pub mod view;
pub(crate) mod window_feedback;

#[cfg(test)]
mod preview_scroll_tests;

pub use surface::{Adapter, Surface};

#[cfg(test)]
mod pb_ipc_fixture;
#[cfg(test)]
mod pb_ipc_tests;
#[cfg(test)]
mod reader_port_a_tests;
#[cfg(test)]
mod reader_port_c_tests;
