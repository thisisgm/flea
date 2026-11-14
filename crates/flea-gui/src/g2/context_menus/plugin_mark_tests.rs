//! A send-target plugin's own SVG leads its menu row, and the named glyph stays until the SVG is read.
use super::lines::{lead, Lead};
use flea::ui_logic_context_menus::{Entry, PluginMark, Text};
use std::sync::Arc;

fn row() -> Entry {
    Entry::leaf(
        "plugin-send:quickshare.send",
        Text::Data("Quick Share".into()),
    )
    .with_glyph("smartphone")
}

#[test]
fn a_read_plugin_mark_leads_the_row_in_place_of_its_glyph() {
    let mark = PluginMark {
        path: "/plugins/quickshare.send/mark.svg".into(),
        svg: Arc::from(&b"<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24'/>"[..]),
    };
    let entry = Entry {
        mark: Some(mark.clone()),
        ..row()
    };
    assert_eq!(lead(&entry), Lead::Plugin(mark));
}

#[test]
fn a_row_whose_mark_is_unread_keeps_its_named_glyph() {
    assert_eq!(lead(&row()), Lead::Cut("smartphone"));
}
