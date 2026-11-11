//! A cursor key held across a pending rename: the re-read keeps the row the cursor is on, and Escape never abandons the write.
use super::{
    queued_navigation_tests::{cursor, hundred_k, key, land_reread, TOTAL},
    rename_tests::{close, editing, receive, sent},
    test_shell::{lists, set_cursor},
};
use crate::backend::Event as BackendEvent;
use gpui::TestAppContext;

// S2-4, v0.3.12 RenameField.qml abandon(): Escape with the write out keeps the editor, so Up still moves nothing.
#[gpui::test]
fn gh265_escape_keeps_a_pending_rename_and_the_held_down_lands_past_it(cx: &mut TestAppContext) {
    let (shell, cx, mut lines, mut all) = hundred_k(cx);
    set_cursor(&shell, cx, 2);
    key(cx, "f2");
    cx.simulate_input("draft");
    key(cx, "enter");
    assert_eq!(sent(&mut lines).len(), 1, "the rename is out");
    key(cx, "up");
    key(cx, "escape");
    assert!(
        editing(&shell, cx).is_some_and(|(row, text, _)| row == 2 && text.starts_with("draft")),
        "Escape keeps the editor and its pending name on the row"
    );
    key(cx, "up");
    assert_eq!(
        cursor(&shell, cx),
        2,
        "Up while the editor holds the rename moves nothing"
    );
    let before = lists(&shell, cx);
    receive(
        &shell,
        cx,
        BackendEvent::Producer(r#"{"t":"renamed","ok":true,"path":"/files/draft.txt"}"#.into()),
    );
    assert_eq!(
        lists(&shell, cx),
        before + 1,
        "the renamed reply reads the folder again"
    );
    assert!(editing(&shell, cx).is_none(), "the reply closes the editor");
    key(cx, "down");
    all[2] = "draft.txt".into();
    land_reread(&shell, cx, "/files", &all, TOTAL, 2);
    assert_eq!(
        cursor(&shell, cx),
        3,
        "Down after the rename lands one row past the renamed file"
    );
    close(&shell, cx);
}

// The same keys without Escape: Up is swallowed while the editor holds the rename, so the cursor stays on the renamed row.
#[gpui::test]
fn gh265_rename_reply_without_escape_lands_the_held_down_past_the_renamed_file(
    cx: &mut TestAppContext,
) {
    let (shell, cx, mut lines, mut all) = hundred_k(cx);
    set_cursor(&shell, cx, 2);
    key(cx, "f2");
    cx.simulate_input("draft");
    key(cx, "enter");
    assert_eq!(sent(&mut lines).len(), 1, "the rename is out");
    key(cx, "up");
    key(cx, "up");
    assert_eq!(
        cursor(&shell, cx),
        2,
        "Up while the editor holds the rename moves nothing"
    );
    let before = lists(&shell, cx);
    receive(
        &shell,
        cx,
        BackendEvent::Producer(r#"{"t":"renamed","ok":true,"path":"/files/draft.txt"}"#.into()),
    );
    assert_eq!(
        lists(&shell, cx),
        before + 1,
        "the renamed reply reads the folder again"
    );
    key(cx, "down");
    all[2] = "draft.txt".into();
    land_reread(&shell, cx, "/files", &all, TOTAL, 2);
    assert_eq!(
        cursor(&shell, cx),
        3,
        "Down after the rename lands one row past the renamed file"
    );
    close(&shell, cx);
}

// A click moves the cursor off the renamed row while the write is pending: the re-read keeps the clicked row.
#[gpui::test]
fn gh265_a_cursor_clicked_off_the_pending_rename_keeps_its_row(cx: &mut TestAppContext) {
    let (shell, cx, mut lines, mut all) = hundred_k(cx);
    set_cursor(&shell, cx, 2);
    key(cx, "f2");
    cx.simulate_input("draft");
    key(cx, "enter");
    assert_eq!(sent(&mut lines).len(), 1, "the rename is out");
    set_cursor(&shell, cx, 1);
    let before = lists(&shell, cx);
    receive(
        &shell,
        cx,
        BackendEvent::Producer(r#"{"t":"renamed","ok":true,"path":"/files/draft.txt"}"#.into()),
    );
    assert_eq!(
        lists(&shell, cx),
        before + 1,
        "the renamed reply reads the folder again"
    );
    key(cx, "down");
    all[2] = "draft.txt".into();
    land_reread(&shell, cx, "/files", &all, TOTAL, 2);
    assert_eq!(
        cursor(&shell, cx),
        2,
        "the held Down moves one row from the clicked row the re-read keeps"
    );
    close(&shell, cx);
}

// A write that lands with no destination of its own (rename-kept) still keeps a cursor moved off the row.
#[gpui::test]
fn gh265_a_moved_cursor_keeps_its_row_when_the_rename_lands_without_a_destination(
    cx: &mut TestAppContext,
) {
    let (shell, cx, mut lines, all) = hundred_k(cx);
    set_cursor(&shell, cx, 2);
    key(cx, "f2");
    cx.simulate_input("draft");
    key(cx, "enter");
    assert_eq!(sent(&mut lines).len(), 1, "the rename is out");
    set_cursor(&shell, cx, 1);
    let before = lists(&shell, cx);
    receive(
        &shell,
        cx,
        BackendEvent::Refused {
            operation: "rename-kept".into(),
            path: String::new(),
            message: "kept".into(),
            mode: 0,
        },
    );
    assert_eq!(
        lists(&shell, cx),
        before + 1,
        "the landed write reads the folder again"
    );
    key(cx, "down");
    land_reread(&shell, cx, "/files", &all, TOTAL, 2);
    assert_eq!(
        cursor(&shell, cx),
        2,
        "the held Down moves one row from the row the re-read keeps"
    );
    close(&shell, cx);
}
