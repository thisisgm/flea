//! Consumer checks enter through the real Shell, backend pipe and IPC dispatch.
#[path = "../../../src/i18n/test_catalogs.rs"]
mod test_catalogs;
// test_catalogs reaches the catalog types through super.
use flea::i18n::{I18n, Paths};
#[path = "m9_selection_preview/geometry.rs"]
mod geometry;
#[path = "m9_selection_preview/host.rs"]
mod host;
#[path = "m9_selection_preview/media.rs"]
mod media;
#[path = "g2_pv2/fixture.rs"]
#[allow(dead_code)]
mod pdf;

#[test]
fn selection_preview_mounts_pdf_media_and_model_in_columns_list_and_grid() {
    host::mounts();
}
#[test]
fn selection_preview_forwards_drawn_column_ipc() {
    host::readers();
}
#[test]
fn selection_preview_aba_drops_stale_reply() {
    host::aba();
}
#[test]
fn selection_preview_view_switch_releases_player() {
    host::releases();
}
#[test]
fn selection_preview_folder_transition_keeps_old_complete_frame() {
    host::folder();
}
#[test]
fn selection_preview_default_keys_reach_the_mounted_preview() {
    host::keys::keys();
}
