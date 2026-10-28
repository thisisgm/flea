//! The column arm of previewTextState: a drawn column code view, its pending frame, and an empty column.
use super::{
    previewtypes_tests::{answer, ask, field, file, local, meta, open, pump},
    test_shell::land,
};
use crate::{
    state::Shell,
    window::{
        selection_preview::{docs::Doc, Mounted},
        u07_tests::read,
    },
};
use flea::jsondoc::{self, Json};
use gpui::{Entity, TestAppContext, VisualTestContext};

fn state(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Json {
    jsondoc::parse(&read(shell, cx, "previewTextState", &["true"])).unwrap()
}

fn reply(token: u64, first: u64, text: &str) -> String {
    format!(
        r#"{{"t":"code","token":{token},"slot":"column","state":"styled","first":{first},"total":1,"lines":[[5,"fn",0," {text}()"]]}}"#
    )
}

#[gpui::test]
fn pb_preview_text_state_column_reads_the_drawn_code_and_its_wrap(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "/files");
    land(&shell, cx, "/files", vec![file("main.rs", "text-x-rust", "Rust source code", 0o100644)], 1);
    let lines = local(&shell, cx, "/files");
    let code = ask(&lines, "code").to_owned();
    meta(&shell, cx, &lines, r#","lines":2"#);
    answer(
        &shell,
        cx,
        &format!(
            r#"{{"t":"code","token":{},"slot":"column","state":"styled","first":{},"total":2,"lines":[[5,"fn",0," drawn()"],[5,"fn",0," second()"]]}}"#,
            field(&code, "token"),
            field(&code, "first")
        ),
    );
    pump(cx);
    let value = state(&shell, cx);
    assert_eq!(value.get("path").and_then(Json::as_str), Some("/files/main.rs"));
    assert_eq!(value.get("ready").and_then(Json::as_bool), Some(true), "the drawn code is ready");
    assert_eq!(value.get("wrap").and_then(Json::as_bool), Some(false));
    assert_eq!(value.get("total").and_then(Json::as_f64), Some(2.));
    assert_eq!(read(&shell, cx, "columnTextLines", &[]), "fn drawn()|fn second()");
    let code = shell.read_with(cx, |shell, cx| match &shell.selection_preview(0).unwrap().read(cx).drawn.as_ref().unwrap().mounted {
        Mounted::Doc(Doc::Code(code)) => code.clone(),
        _ => panic!("the column draws code"),
    });
    code.update(cx, |code, cx| code.set_wrap(true, cx));
    pump(cx);
    assert_eq!(state(&shell, cx).get("wrap").and_then(Json::as_bool), Some(true), "the live wrap key");
}

#[gpui::test]
fn pb_preview_text_state_column_is_not_ready_while_a_pending_frame_waits(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "/files");
    let rows = vec![
        file("a.rs", "text-x-rust", "Rust source code", 0o100644),
        file("b.rs", "text-x-rust", "Rust source code", 0o100644),
    ];
    land(&shell, cx, "/files", rows, 1);
    let lines = local(&shell, cx, "/files");
    let code = ask(&lines, "code").to_owned();
    meta(&shell, cx, &lines, r#","lines":1"#);
    answer(&shell, cx, &reply(field(&code, "token"), field(&code, "first"), "drawn"));
    pump(cx);
    let value = state(&shell, cx);
    assert_eq!(value.get("path").and_then(Json::as_str), Some("/files/a.rs"));
    assert_eq!(value.get("ready").and_then(Json::as_bool), Some(true));
    cx.simulate_keystrokes("down");
    let lines = pump(cx);
    let next = ask(&lines, "code").to_owned();
    answer(&shell, cx, &reply(field(&next, "token"), field(&next, "first"), "pending"));
    pump(cx);
    let value = state(&shell, cx);
    assert_eq!(value.get("path").and_then(Json::as_str), Some("/files/a.rs"), "the drawn view stays a.rs");
    assert_eq!(value.get("ready").and_then(Json::as_bool), Some(false), "b.rs waits on its meta, so a.rs is not ready");
    meta(&shell, cx, &lines, r#","lines":1"#);
    pump(cx);
    let value = state(&shell, cx);
    assert_eq!(value.get("path").and_then(Json::as_str), Some("/files/b.rs"));
    assert_eq!(value.get("ready").and_then(Json::as_bool), Some(true), "b.rs lands and is ready");
}

#[gpui::test]
fn pb_preview_text_state_column_answers_the_empty_shape_before_any_frame(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "/files");
    land(&shell, cx, "/files", vec![file("a.rs", "text-x-rust", "Rust source code", 0o100644)], 1);
    assert_eq!(read(&shell, cx, "previewTextState", &["true"]), "{}");
}
