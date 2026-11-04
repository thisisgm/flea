//! keyDeliveryState's preview field reads the Columns preview column the production Shell draws and focuses.
use super::previewtypes_tests::{answer, ask, field, file, local, meta, open, pump};
use crate::window::u07_tests::read;
use flea::jsondoc::{self, Json};
use gpui::TestAppContext;

fn delivery(shell: &gpui::Entity<crate::state::Shell>, cx: &mut gpui::VisualTestContext) -> Json {
    jsondoc::parse(&read(shell, cx, "keyDeliveryState", &[])).expect("keyDeliveryState is JSON")
}

#[gpui::test]
fn key_delivery_state_reads_the_drawn_and_focused_column_preview(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "/files");
    let rows = vec![file("main.rs", "text-x-rust", "Rust source code", 0o100644)];
    super::test_shell::land(&shell, cx, "/files", rows, 1);
    let lines = local(&shell, cx, "/files");
    let code = ask(&lines, "code").to_owned();
    meta(&shell, cx, &lines, r#","lines":1"#);
    answer(
        &shell,
        cx,
        &format!(
            r#"{{"t":"code","token":{},"slot":"column","state":"styled","first":{},"total":1,"lines":[[5,"fn",0," main()"]]}}"#,
            field(&code, "token"),
            field(&code, "first")
        ),
    );
    pump(cx);
    assert_eq!(read(&shell, cx, "previewColumnState", &[]), "code");
    let drawn = delivery(&shell, cx);
    assert_eq!(
        drawn.get("preview").unwrap(),
        &jsondoc::parse(r#"{"index":0,"focused":false}"#).unwrap()
    );
    assert_eq!(drawn.get("listFocus").unwrap(), &Json::Bool(true));
    cx.simulate_keystrokes("ctrl-tab");
    pump(cx);
    let focused = delivery(&shell, cx);
    assert_eq!(
        focused.get("preview").unwrap(),
        &jsondoc::parse(r#"{"index":0,"focused":true}"#).unwrap()
    );
    assert_eq!(
        focused.get("activeFocusItem").unwrap(),
        &Json::Str("SelectionPreview".into())
    );
    assert_eq!(
        focused.get("paneFocus").unwrap(),
        &Json::Bool(true),
        "the column preview is inside the pane"
    );
    assert_eq!(focused.get("listFocus").unwrap(), &Json::Bool(false));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
