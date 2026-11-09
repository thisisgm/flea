//! G2 surface families; each unit keeps its GUI half under its own module.
pub mod archive_flash;
pub mod context_menus;
pub mod custom_actions;
pub mod device_rail;
pub mod doc_preview;
pub mod drag_drop;
#[cfg(test)]
mod error_ink_tests;
pub mod file_clipboard;
pub mod file_ops;
pub mod file_search;
pub mod listing_filter;
pub mod media_preview;
pub mod model_preview;
pub mod network_shares;
pub mod open_with_properties;
pub mod picker_listing;
pub mod picker_portal;
pub mod plugin_settings;
pub(crate) mod plugins;
pub mod print_convert;
pub mod quick_look_overlay;
pub mod rail_places;
pub mod rclone_send;
pub mod send_share;
pub mod settings_pages;
pub mod status_feedback;
pub mod trash_browser;
