use super::readers3_tests::{open, read, update};
use flea::{jsondoc, jsondoc::Json, ui_logic_plugin_settings::contributions::ActionInput};
use gpui::TestAppContext;

#[gpui::test]
fn plugin_action_recovery_input_reader_reports_value_selection_and_caret(cx: &mut TestAppContext) {
    let handle = open(cx);
    update(cx, handle, |shell, window, cx| {
        shell
            .plugin_text_input(
                &ActionInput {
                    key: "checksum".into(),
                    title: "Verify Checksum".into(),
                    label: "Checksum".into(),
                    placeholder: String::new(),
                    submit: "Verify".into(),
                    max_chars: 128,
                },
                window,
                cx,
            )
            .unwrap();
        shell.rename.ime.entry.ime_replace(None, "éabc");
        shell.dialog_edited(cx);
        shell.rename.ime.entry.move_to(2);
        shell.rename.ime.entry.select_to(4);
    });
    let before = update(cx, handle, |shell, _, _| {
        (
            shell.rename.ime.entry.generation,
            shell.rename.ime.entry.text.clone(),
            shell.rename.ime.entry.selected.clone(),
        )
    });
    let answer = read(cx, handle, "menuDialogState", &[]);
    assert_eq!(answer.code, 0);
    let doc = jsondoc::parse(answer.stdout.trim()).unwrap();
    assert_eq!(doc.get("opened"), Some(&Json::Bool(true)));
    assert_eq!(doc.get("value").and_then(Json::as_str), Some("éabc"));
    assert_eq!(
        doc.get("selection"),
        Some(&Json::Arr(vec![
            Json::Num("2".into()),
            Json::Num("4".into())
        ]))
    );
    assert_eq!(doc.get("caret").and_then(Json::as_f64), Some(4.));
    assert_eq!(
        doc.get("offsetUnit").and_then(Json::as_str),
        Some("utf8-bytes")
    );
    assert_eq!(read(cx, handle, "menuDialogState", &[]), answer);
    update(cx, handle, |shell, _, _| {
        assert_eq!(
            before,
            (
                shell.rename.ime.entry.generation,
                shell.rename.ime.entry.text.clone(),
                shell.rename.ime.entry.selected.clone()
            )
        )
    });
}
