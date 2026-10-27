//! Closing a card cannot route its late callback into an inline editor that was painted underneath it.
use super::{draw, input, ready};
use gpui::{EntityInputHandler, TestAppContext};
#[gpui::test]
fn i50_dialog_closed_callback_refuses_underlying_name(cx: &mut TestAppContext) {
    let (shell, cx, _) = ready(cx);
    input(cx, &shell, |s, w, cx| {
        s.rename_open(0, 0, w, cx);
    });
    draw(cx);
    input(cx, &shell, |s, _, _| {
        assert!(s
            .rename
            .edit
            .as_ref()
            .unwrap()
            .line
            .line
            .registered
            .is_some());
        s.operations
            .archive
            .extract
            .open("/files/a.7z", "/files", "", &s.locale.catalog);
    });
    draw(cx);
    input(cx, &shell, |s, w, cx| {
        assert!(s
            .rename
            .edit
            .as_ref()
            .unwrap()
            .line
            .line
            .registered
            .is_none());
        let before = s.rename.edit.as_ref().unwrap().entry.text.clone();
        s.operations.archive.extract.close();
        s.sync_dialog_editor();
        s.replace_text_in_range(None, "late", w, cx);
        assert_eq!(s.rename.edit.as_ref().unwrap().entry.text, before);
        assert!(s.selected_text_range(false, w, cx).is_none());
    });
    draw(cx);
    cx.simulate_input("fresh");
    assert_eq!(
        shell.read_with(cx, |s, _| s
            .rename
            .edit
            .as_ref()
            .unwrap()
            .entry
            .text
            .clone()),
        "fresh"
    );
}
