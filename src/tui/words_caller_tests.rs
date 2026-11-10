use super::tests::Fixture;
use crate::{
    jsondoc,
    tui::{render::cards, wire},
};

#[test]
fn tui_words_properties_receive_and_refresh_keep_original_facts() {
    let fixture = Fixture::new();
    fixture.write("catalog/ja/flea.json", r#"{"properties.size":"容量"}"#);
    let mut model = fixture.model();
    model.action_id = 11;
    model.sheet = true;
    model.properties = Some(Vec::new());
    let (mut backend, reader) = wire::echo_wire();
    let reply = jsondoc::parse(r#"{"t":"menuaction","op":"properties","id":11,"ok":true,"path":"/fx/تقرير_2025.txt","kind":"raw kind","bytes":1200,"mode":"0644","owner":"raw owner","uid":1000,"gid":1000}"#).unwrap();
    model.receive(reply, &mut backend).unwrap();
    let before = model.properties.clone().unwrap();
    assert!(before
        .iter()
        .any(|(_, value)| value == "/fx/تقرير_2025.txt"));
    fixture.write("language", "ja\n");
    assert!(model.refresh_locale(&[cards::refresh_locale]));
    let after = model.properties.as_ref().unwrap();
    assert!(after.iter().any(|(key, _)| key == "容量"));
    assert!(after.iter().any(|(_, value)| value == "/fx/تقرير_2025.txt"));
    assert_eq!(model.action_id, 11);
    assert!(
        backend.events.try_recv().is_err(),
        "refresh sends no backend request"
    );
    wire::finish(backend, reader);
}

fn name_fixture() -> Fixture {
    let fixture = Fixture::new();
    fixture.write("catalog/en/flea.json", r#"{
        "tui.editor.invalid_name":"Enter a name that is not empty, . or .., and contains neither / nor a null character.",
        "tui.editor.name_empty":"A name cannot be empty.",
        "tui.editor.name_dots":"A name cannot be a single dot or a double dot.",
        "tui.editor.name_slash":"A name cannot contain /.",
        "tui.editor.name_nul":"A name cannot contain a null character."
    }"#);
    fixture.write(
        "catalog/ja/flea.json",
        r#"{
        "tui.editor.invalid_name":"無効な名前",
        "tui.editor.name_empty":"空の名前"
    }"#,
    );
    fixture
}

#[test]
fn tui_words_name_faults_keep_specific_shared_validation_detail() {
    let fixture = name_fixture();
    let mut model = fixture.model();
    let theme = crate::tui::theme::Theme::from_text("");
    for (name, expected) in [
        ("", "A name cannot be empty."),
        (".", "A name cannot be a single dot or a double dot."),
        ("..", "A name cannot be a single dot or a double dot."),
        ("a/b", "A name cannot contain /."),
        ("a\0b", "A name cannot contain a null character."),
    ] {
        let mut editor = crate::tui::editor::Editor::new("mkdir", name.into(), "/fx".into());
        assert!(!editor.valid_localized(&model.i18n));
        assert_eq!(editor.error, expected);
        assert_eq!(editor.value, name);
        model.editor = Some(editor);
        let (row, _) = crate::tui::prompts::status(&model, &theme, 130, std::time::Duration::ZERO);
        assert!(crate::tui::render::frame_tests::plain(&row).contains(expected));
    }
}

// A 64-column terminal leaves 2 cells once the four-line rule is drawn whole, so the rule gives way instead.
#[test]
fn tui_words_create_prompt_keeps_its_field_on_a_narrow_row() {
    let fixture = name_fixture();
    let mut model = fixture.model();
    let theme = crate::tui::theme::Theme::from_text("");
    model.editor = Some(crate::tui::editor::Editor::new(
        "mkdir",
        "New Folder/".into(),
        "/fx".into(),
    ));
    for columns in [64, 80, 130] {
        let (row, caret) =
            crate::tui::prompts::status(&model, &theme, columns, std::time::Duration::ZERO);
        let plain = crate::tui::render::frame_tests::plain(&row);
        assert_eq!(
            crate::tui::cells::text_width(&plain),
            columns,
            "{columns}: the row fills the terminal"
        );
        assert!(
            plain.contains("New Folder/"),
            "{columns}: the name and its caret survive: {plain:?}"
        );
        assert!(caret.is_some(), "{columns}: the caret has a column");
    }
}

#[test]
fn tui_words_name_backend_error_keeps_data_identity() {
    let fixture = name_fixture();
    let mut model = fixture.model();
    model.editor = Some(crate::tui::editor::Editor::new(
        "mkdir",
        String::new(),
        "/fx".into(),
    ));
    assert!(!model.editor.as_mut().unwrap().valid_localized(&model.i18n));
    let diagnostic = model.editor.as_ref().unwrap().error.clone();
    let reply = crate::jsondoc::Json::Obj(vec![
        ("t".into(), wire::word("error")),
        ("where".into(), wire::word("mkdir")),
        ("msg".into(), wire::word(&diagnostic)),
    ]);
    let (mut backend, reader) = wire::echo_wire();
    model.receive(reply, &mut backend).unwrap();
    wire::finish(backend, reader);
    fixture.write("language", "ja\n");
    assert!(model.refresh_locale(&[cards::refresh_locale]));
    assert_eq!(model.editor.as_ref().unwrap().error, diagnostic);
}
