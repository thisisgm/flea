use super::sheet::{Sheet, Sources};
use super::tests::english;
use flea::keys::display::KeycapSnapshot;

#[test]
fn keymap_plus_sheet_draws_and_queries_board_labels() {
    for (action, label) in [
        ("textSizeUp", "ctrl-shift-plus"),
        ("textSizeDown", "ctrl-shift-minus"),
        ("zoomIn", "+"),
        ("zoomOut", "-"),
    ] {
        let mut sheet = Sheet::new("default", english(), Sources::default());
        sheet.set_keycaps(Some(KeycapSnapshot::new("us", None, None, 1).unwrap()));
        let row = sheet.rows.iter().find(|row| row.action == action).unwrap();
        assert_eq!(sheet.cap(&row.keys), label, "{action}");
        sheet.query_changed(label.to_owned());
        let index = sheet
            .ranked
            .iter()
            .position(|row| row.action == action)
            .unwrap();
        assert_eq!(sheet.result_keys(index), label, "query {label}");
    }
}
