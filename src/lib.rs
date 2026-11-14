//! Shared, dependency-free protocol primitives for Flea frontends.
extern crate self as flea;

pub mod bytename;
pub mod chooser_startup;
pub mod colours;
pub mod desktop_font;
pub mod error;
pub mod executable_file;
pub mod favourites;
pub mod i18n;
pub mod ipc;
pub mod json;
pub mod jsondoc;
mod jsonstring;
pub mod keys;
pub mod oflags;
pub mod ui_logic_preview_core;
pub mod uischema;
pub mod uistate;
pub mod uistore;
// The library's state store checks only the ui shape; the binary reads every other document.
#[allow(dead_code)]
mod persistdoc;
pub mod plugin_runtime;
pub mod preview_model;
pub mod protocol;
pub mod qscrash;
pub mod rawname;
pub mod textdecode;
pub mod theme;
pub mod ui_logic_archive_flash;
pub mod ui_logic_context_menus;
pub mod ui_logic_custom_actions;
pub mod ui_logic_device_rail;
pub mod ui_logic_doc_preview;
pub mod ui_logic_drag_drop;
pub mod ui_logic_file_clipboard;
pub mod ui_logic_file_ops;
pub mod ui_logic_file_search;
pub mod ui_logic_listing_filter;
pub mod ui_logic_media_preview;
pub mod ui_logic_model_preview;
pub mod ui_logic_network_shares;
pub mod ui_logic_open_with_properties;
pub mod ui_logic_picker_listing;
pub mod ui_logic_picker_portal;
pub mod ui_logic_plugin_settings;
pub mod ui_logic_print_convert;
pub mod ui_logic_rail_places;
pub mod ui_logic_rclone_send;
pub mod ui_logic_send_share;
pub mod ui_logic_settings_pages;
pub mod ui_logic_status_feedback;
pub mod ui_logic_trash_browser;
mod uimigrate;
mod uiplugins;
pub mod userfile;

// The catalog and locale API shares the backend's regular-file and wall-clock primitives.
pub mod backend {
    #[cfg(test)]
    pub mod fifotest;
    #[cfg(test)]
    pub mod nearbyrows;
    pub mod netfs;
    pub mod networkaddress;
    // Quick Look reads a picture's header beside its meta ask, so the picture draws before the reply.
    pub mod imagesize;
    pub mod rastertemp;
    pub mod regfile;
    #[cfg(test)]
    pub mod testdir;
    pub mod trashdate;
}
