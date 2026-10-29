//! Preview parity through the mounted Root, real keys and live IPC readers.
use super::{
    preview_tests::{answer, close, code_reply, opened, press, rows, sent, token},
    test_shell::{land, set_cursor},
};
use crate::window::u07_tests::read;
use flea::jsondoc::{self, Json};
use gpui::TestAppContext;

#[gpui::test]
fn pb_preview_text_state_tracks_the_live_code_and_wrap_key(cx: &mut TestAppContext) {
    let (shell, cx, mut requests) = opened(cx);
    land(&shell, cx, "/files", rows(), 1);
    set_cursor(&shell, cx, 1);
    press(cx, "space");
    let ask = sent(&mut requests, "code");
    answer(
        &shell,
        cx,
        code_reply(token(&ask[0]), &["first", "second"], 2),
    );
    let value = jsondoc::parse(&read(&shell, cx, "previewTextState", &["false"])).unwrap();
    assert_eq!(
        value.get("path").and_then(Json::as_str),
        Some("/files/notes.txt")
    );
    assert_eq!(value.get("ready").and_then(Json::as_bool), Some(true));
    assert_eq!(value.get("wrap").and_then(Json::as_bool), Some(false));
    assert_eq!(value.get("total").and_then(Json::as_f64), Some(2.));
    press(cx, "w");
    let value = jsondoc::parse(&read(&shell, cx, "previewTextState", &["false"])).unwrap();
    assert_eq!(value.get("wrap").and_then(Json::as_bool), Some(true));
    press(cx, "escape");
    assert_eq!(read(&shell, cx, "previewTextState", &["false"]), "{}");
    close(&shell, cx);
}
