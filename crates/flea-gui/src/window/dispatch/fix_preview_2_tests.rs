//! FIX-PREVIEW-2: e on a shown column image is quiet (EY-518).
use super::previewtypes_kinds_tests::Root;
use super::previewtypes_tests::{answer, file, local, meta, open, pump};
use crate::window::u07_tests::read;
use gpui::TestAppContext;

const REFUSAL: &str = "That action is not available yet";

#[gpui::test]
fn fix_preview_2_e_on_a_column_image_is_quiet_focused_or_not(cx: &mut TestAppContext) {
    // The picture decodes on the asset loader's thread, outside the deterministic scheduler.
    cx.executor().allow_parking();
    let root = Root::new("fixpv2-expand");
    let dir = root.0.to_string_lossy().into_owned();
    std::fs::write(root.0.join("image-png.png"), b"not a picture").unwrap();
    let (shell, cx) = open(cx, &dir);
    let rows = vec![file("image-png.png", "image-png", "PNG image", 0o100644)];
    super::test_shell::land(&shell, cx, &dir, rows, 1);
    let lines = local(&shell, cx, &dir);
    meta(&shell, cx, &lines, r#","w":2,"h":2"#);
    answer(&shell, cx, r#"{"t":"thumbed","row":0,"file":""}"#);
    pump(cx);
    let shown = shell.read_with(cx, |shell, cx| {
        shell
            .selection_preview(0)
            .is_some_and(|preview| preview.read(cx).visible)
    });
    assert!(shown, "the column shows the image");
    // The listing holds the keyboard first, as after the drag in the EY-518 scene; then the focused column.
    use flea::keys::Context;
    for (keys, context) in [("e", Context::Listing), ("ctrl-tab e", Context::Preview)] {
        cx.simulate_keystrokes(keys);
        pump(cx);
        assert_eq!(
            super::preview_tests::context(&shell, cx),
            context,
            "{keys} runs with the keyboard there"
        );
        assert_ne!(
            read(&shell, cx, "lastMessage", &[]),
            REFUSAL,
            "{keys} is quiet"
        );
        assert_eq!(
            read(&shell, cx, "cursor", &[]),
            "0",
            "{keys} leaves the row"
        );
    }
}

// EY-570: Preview.qml toggleWrap writes preview.wrap, so the next Quick Look opens the way w left it.
#[gpui::test]
fn fix_preview_2_quick_look_wrap_toggle_persists_across_a_reopen(cx: &mut TestAppContext) {
    use super::preview_tests::{answer, code_reply, file, opened, press, rows, sent, token};
    use flea::jsondoc::{self, Json};
    let (shell, cx, mut requests) = opened(cx);
    let mut listed = rows();
    listed.push(file("other.txt", "text-plain"));
    super::test_shell::land(&shell, cx, "/files", listed.clone(), 1);
    super::test_shell::set_cursor(&shell, cx, 1);
    let wrap = |cx: &mut gpui::VisualTestContext| {
        let state = jsondoc::parse(&read(&shell, cx, "previewTextState", &["false"])).unwrap();
        state.get("wrap").and_then(Json::as_bool)
    };
    press(cx, "space");
    let ask = sent(&mut requests, "code");
    answer(
        &shell,
        cx,
        code_reply(token(&ask[0]), &["first", "second"], 2),
    );
    assert_eq!(wrap(cx), Some(false), "the first card opens unwrapped");
    press(cx, "w");
    assert_eq!(wrap(cx), Some(true));
    let saved = shell.read_with(cx, |shell, _| {
        shell
            .launch
            .settings
            .data
            .get("preview")
            .and_then(|preview| preview.get("wrap"))
            .and_then(Json::as_bool)
    });
    assert_eq!(saved, Some(true), "w saves preview.wrap");
    // This fixture's ui.json lacks the defaults, so the save relists the pane.
    super::test_shell::land(&shell, cx, "/files", listed, 2);
    press(cx, "escape");
    // Another file, so the second card is a fresh mount that can only take wrap from ui.json.
    super::test_shell::set_cursor(&shell, cx, 4);
    press(cx, "space");
    let ask = sent(&mut requests, "code");
    answer(
        &shell,
        cx,
        code_reply(token(&ask[0]), &["first", "second"], 2),
    );
    assert_eq!(wrap(cx), Some(true), "the next card opens wrapped");
}
