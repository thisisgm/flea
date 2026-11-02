use crate::{
    jsondoc,
    tui::{icons, pane, render::frame_tests, theme::Theme, wire},
};

#[test]
fn tui_parity17_hig_properties_uses_the_listings_mime_description() {
    let mut m = frame_tests::model();
    m.sheet = true;
    m.action_id = 8;
    m.properties = Some(Vec::new());
    let (mut w, reader) = wire::echo_wire();
    let reply = jsondoc::parse(r#"{"t":"menuaction","op":"properties","id":8,"ok":true,"path":"/fx/v/work/notes.txt","kind":"File","bytes":99,"mode":"0644"}"#).unwrap();
    m.receive(reply, &mut w).unwrap();
    assert!(m
        .properties
        .as_ref()
        .unwrap()
        .iter()
        .any(|(key, value)| key == "Kind" && value == "Plain text document"));
    m.rows.clear();
    crate::tui::render::cards::refresh_locale(&mut m);
    assert!(m
        .properties
        .as_ref()
        .unwrap()
        .iter()
        .any(|(key, value)| key == "Kind" && value == "Plain text document"));
    wire::finish(w, reader);
}

#[test]
fn properties_keep_the_shared_folder_link_and_special_descriptions() {
    let i18n = crate::i18n::test_catalogs::english();
    for (kind, directory, expected) in [
        ("Directory", true, "Folder"),
        ("Symbolic link", false, "Symbolic link"),
        ("Special file", false, "Special file"),
    ] {
        let reply = jsondoc::parse(&format!(
            r#"{{"kind":"{kind}","directory":{directory},"mimeKind":"Plain text document"}}"#
        ))
        .unwrap();
        let facts = crate::tui::render::cards::properties(&reply, &i18n);
        assert!(facts
            .iter()
            .any(|(key, value)| key == "Kind" && value == expected));
    }
}

#[test]
fn properties_use_the_selected_identity_outside_the_visible_window() {
    let mut m = frame_tests::model();
    let selected = m.rows.remove(&7).unwrap();
    m.selected.insert(7);
    m.selected_rows.insert(7, selected);
    m.cursor = 8;
    m.sheet = true;
    m.action_id = 8;
    m.properties = Some(Vec::new());
    let (mut w, reader) = wire::echo_wire();
    m.receive(jsondoc::parse(r#"{"t":"menuaction","op":"properties","id":8,"ok":true,"path":"/fx/v/work/notes.txt","kind":"File"}"#).unwrap(), &mut w).unwrap();
    assert!(m
        .properties
        .as_ref()
        .unwrap()
        .iter()
        .any(|(key, value)| key == "Kind" && value == "Plain text document"));
    wire::finish(w, reader);
}

#[test]
fn tui_parity17_hig_binary_data_keeps_the_binary_glyph_in_list_and_preview() {
    let mut m = frame_tests::model();
    let mut blob = frame_tests::entry("blob.dat", false, 25);
    blob.kind = crate::backend::rows::UNKNOWN_KIND.into();
    assert_eq!(icons::row(&blob, true), "\u{eae8}");
    m.rows.insert(7, blob);
    m.preview_path = m.path.join("blob.dat");
    m.font_marks = true;
    let theme = Theme::from_text("");
    let head = pane::preview(&m, 0, 50, 27, &theme);
    assert!(head.contains("\u{eae8}"));
    assert!(!head.contains("\u{ec5e}"));
    assert_eq!(
        icons::row(&frame_tests::entry("notes.txt", false, 99), true),
        icons::TEXT
    );
}
