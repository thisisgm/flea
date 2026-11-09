//! The one place the window runs what a producer hands it: key and pointer actions, menu choices and rail requests.
mod clipboard;
mod deletion;
#[cfg(test)]
mod document_keys_fixture;
#[cfg(test)]
mod document_keys_tests;
#[cfg(test)]
mod ds_ql_source_carry_tests;
mod fileops;
mod fileops_created;
mod fileops_wire;
mod listing;
mod menu_parity;
mod menus;
mod navigation;
pub(crate) mod opener;
mod original;
mod places;
mod preview;
mod previewtypes;
#[cfg(test)]
mod properties_close_tests;
#[cfg(test)]
mod pvset_repro_tests;
pub(crate) mod rail_menu;
#[cfg(test)]
mod recent_parity_tests;
#[cfg(test)]
mod recent_tabs_remainder_tests;
mod rename;
#[cfg(test)]
mod s5_preview_tests;
#[cfg(test)]
mod tab_sort_memory_tests;
mod tabs;
mod tag_capability;
mod tag_places;
mod tags;
mod window;

use super::input::ActionEvent;
use crate::{g2::context_menus::join::Chosen, state::Shell};
use gpui::{Context, Window};

pub(crate) use fileops::Held as FileOps;

mod order;
pub(crate) use order::Order;

/// A family runs the orders it owns and answers whether it ran this one.
type Family = fn(&mut Shell, &Order, &mut Window, &mut Context<Shell>) -> bool;

/// Each family keeps its handlers in its own file beside this one and adds one line here; file operations go first, to end an armed dd pair on every other listing action.
const FAMILIES: &[Family] = &[
    fileops::run,
    tag_capability::run,
    navigation::run,
    tabs::run,
    tag_places::run,
    opener::run,
    original::run,
    places::run,
    previewtypes::run,
    preview::run,
    listing::run,
    window::run,
    super::permissions::run,
    rename::run,
    menu_parity::run,
    menus::run,
    rail_menu::run,
    tags::run,
    clipboard::run,
];

mod run;

#[cfg(test)]
mod band_shift_tests;
#[cfg(test)]
mod band_tests;
#[cfg(test)]
mod clipboard_tests;
#[cfg(test)]
mod columns_markdown_reader_tests;
#[cfg(test)]
mod compare_tests;
#[cfg(test)]
mod dual_keys_tests;
#[cfg(test)]
mod effect_tests;
#[cfg(test)]
mod families_tests;
#[cfg(test)]
mod fileops_arrival_tests;
#[cfg(test)]
mod fileops_menu_tests;
#[cfg(test)]
mod fileops_new_file_tests;
#[cfg(test)]
mod fileops_tests;
#[cfg(test)]
mod filterkeys_tests;
#[cfg(test)]
mod fix_fileops_2_tests;
#[cfg(test)]
mod fix_preview_2_tests;
#[cfg(test)]
mod fixcards_compare_tests;
#[cfg(test)]
mod fixchrome_ey522_tests;
#[cfg(test)]
mod fixmenus_hidden_tests;
#[cfg(test)]
mod fixmenus_tests;
#[cfg(test)]
mod frame_gate_tests;
#[cfg(test)]
mod gh_rename_proof_lane;
#[cfg(test)]
mod gh_rename_proof_tests;
#[cfg(test)]
mod grid_select_tests;
#[cfg(test)]
mod guard_tests;
#[cfg(test)]
mod key_delivery_preview_tests;
#[cfg(test)]
mod key_rows_focus_tests;
#[cfg(test)]
mod key_rows_tests;
#[cfg(test)]
mod key_rows_trash_reload_tests;
#[cfg(test)]
mod key_rows_trash_tests;
#[cfg(test)]
mod listing_marks_tests;
#[cfg(test)]
mod listing_pointer_keep_tests;
#[cfg(test)]
mod listing_pointer_tests;
#[cfg(test)]
mod listing_tests;
#[cfg(test)]
mod live_md_chrome_tests;
#[cfg(test)]
mod menudeliver_openwith_tests;
#[cfg(test)]
mod menudeliver_props_tests;
#[cfg(test)]
mod menudeliver_tests;
#[cfg(test)]
mod menudraw_tests;
#[cfg(test)]
mod menus3_addon_tests;
#[cfg(test)]
mod menus3_tests;
#[cfg(test)]
mod menus_columns_tests;
#[cfg(test)]
mod menus_dual_tests;
#[cfg(test)]
mod menus_keys_tests;
#[cfg(test)]
mod menus_tests;
#[cfg(test)]
mod mounted_navigation_tests;
#[cfg(test)]
mod navigation_tests;
#[cfg(test)]
mod network_parity_tests;
#[cfg(test)]
mod opener_focus_tests;
#[cfg(test)]
mod opener_menu_tests;
#[cfg(test)]
mod opener_selection_tests;
#[cfg(test)]
mod opener_tests;
#[cfg(test)]
mod openwith_card_icons_tests;
#[cfg(test)]
mod openwith_controls_tests;
#[cfg(test)]
mod openwith_icon_tests;
#[cfg(test)]
mod openwith_multi_tests;
#[cfg(test)]
mod openwith_painted_tests;
#[cfg(test)]
mod openwith_tests;
#[cfg(test)]
mod openwith_wheel_tests;
#[cfg(test)]
mod pb_fileops_tests;
#[cfg(test)]
mod pb_menus_2_tests;
#[cfg(test)]
mod pb_opener_tests;
#[cfg(test)]
mod pb_path_tests;
#[cfg(test)]
pub(crate) mod pb_places_tests;
#[cfg(test)]
mod pb_wire_tests;
#[cfg(test)]
mod presetkeys_tests;
#[cfg(test)]
mod preview_quality_own_size_tests;
#[cfg(test)]
mod preview_quality_tests;
#[cfg(test)]
mod preview_quiet_keys_tests;
#[cfg(test)]
mod preview_reuse_tests;
#[cfg(test)]
mod preview_tests;
#[cfg(test)]
mod previewtypes_cover_tests;
#[cfg(test)]
mod previewtypes_fit_tests;
#[cfg(test)]
mod previewtypes_fixture_tests;
#[cfg(test)]
mod previewtypes_gif_tests;
#[cfg(test)]
mod previewtypes_hold_tests;
#[cfg(test)]
mod previewtypes_kinds_tests;
#[cfg(test)]
mod previewtypes_page_tests;
#[cfg(test)]
mod previewtypes_svgfit_tests;
#[cfg(test)]
mod previewtypes_tests;
#[cfg(test)]
mod pvcode_tests;
#[cfg(test)]
mod queued_navigation_escape_tests;
#[cfg(test)]
mod queued_navigation_tests;
#[cfg(test)]
mod quick_look_body_reply_tests;
#[cfg(test)]
mod quick_look_body_tests;
#[cfg(test)]
mod quick_look_calm_tests;
#[cfg(test)]
mod quick_look_chrome_tests;
#[cfg(test)]
mod quick_look_clock_tests;
#[cfg(test)]
mod quick_look_font_tests;
#[cfg(test)]
mod quick_look_ground_tests;
#[cfg(test)]
mod quick_look_media_tests;
#[cfg(test)]
mod quick_look_reply_tests;
#[cfg(test)]
mod quick_look_sheet_tests;
#[cfg(test)]
mod quick_look_state_tests;
#[cfg(test)]
mod refusal_tests;
#[cfg(test)]
mod rename_bulk_tests;
#[cfg(test)]
mod rename_panes_tests;
#[cfg(test)]
mod rename_refusal_tests;
#[cfg(test)]
mod rename_slow_tests;
#[cfg(test)]
mod rename_tests;
#[cfg(test)]
mod rename_views_tests;
#[cfg(test)]
mod rename_watch_tests;
#[cfg(test)]
mod retry_tests;
#[cfg(test)]
mod runcard_tests;
#[cfg(test)]
mod search_enter_reveal_tests;
#[cfg(test)]
mod search_open_tests;
#[cfg(test)]
mod tabs_tests;
#[cfg(test)]
pub(crate) mod test_shell;
#[cfg(test)]
mod trash_ipc_tests;
#[cfg(test)]
mod trash_parity_tests;
#[cfg(test)]
mod u_settings_tests;
#[cfg(test)]
mod vf_more_refresh_tests;
#[cfg(test)]
mod vf_more_tests;

#[cfg(test)]
mod tags_races_tests;
#[cfg(test)]
mod tags_routes_tests;
#[cfg(test)]
mod tags_tests;

#[cfg(test)]
mod gh_preview_proof_tests;
#[cfg(test)]
mod pb_grid_band_tests;
#[cfg(test)]
mod pb_grid_tap_tests;
#[cfg(test)]
mod pb_grid_tests;
#[cfg(test)]
mod pb_preview_fixture;
#[cfg(test)]
mod pb_preview_keys_tests;
#[cfg(test)]
mod pb_preview_readers_tests;
#[cfg(test)]
mod pc2_reader_tests;
#[cfg(test)]
mod rail_menu_tests;

#[cfg(test)]
mod pb_pane_feedback_tests;
#[cfg(test)]
mod pb_pane_state_tests;

#[cfg(test)]
mod tag_capability_tests;

#[cfg(test)]
mod columns_search_parity_tests;

#[cfg(test)]
mod pb_rest_tests;

#[cfg(test)]
mod pb_rest_preview_tests;

#[cfg(test)]
mod pb_rest_prepare_tests;

#[cfg(test)]
mod parity_preview_column_tests;
#[cfg(test)]
mod parity_preview_switch_tests;
#[cfg(test)]
mod parity_preview_tests;
#[cfg(test)]
mod railrecent_tests;

#[cfg(test)]
mod railrecent_owner_tests;

#[cfg(test)]
mod columns_preview_readers_tests;
#[cfg(test)]
mod pb_rest_2_tests;
#[cfg(test)]
mod pb_tabs_overlay_tests;
#[cfg(test)]
mod railrecent_dual_tests;

#[cfg(test)]
mod gm_prevsel_tests;

#[cfg(test)]
mod outline_tests;

#[cfg(test)]
mod outline_frame_tests;
#[cfg(test)]
mod outline_hover_tests;
#[cfg(test)]
mod outline_mode_tests;
#[cfg(test)]
mod vf5_tests;

#[cfg(test)]
mod vf5_git_tests;

#[cfg(test)]
mod quick_look_plugin_tests;
#[cfg(test)]
mod rtl_mirror_tests;
#[cfg(test)]
mod vf5_zoom_tests;

#[cfg(test)]
mod thumbnail_policy_tests;

#[cfg(test)]
mod quick_look_interim_tests;

#[cfg(test)]
mod s4_tests;

#[cfg(test)]
mod trash_navigation_tests;
