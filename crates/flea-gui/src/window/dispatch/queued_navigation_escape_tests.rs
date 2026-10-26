//! A cursor key held across an Escaped rename: the re-read keeps the row the cursor moved to, and a plain Enter rename keeps the renamed row.
use super::{
    queued_navigation_tests::{cursor, hundred_k, key, land_reread, TOTAL},
    rename_tests::{close, receive, sent},
    test_shell::{lists, set_cursor},
};
use crate::backend::Event as BackendEvent;
use gpui::TestAppContext;

// F2, Enter, Escape, then Up moves the cursor off the renamed row: the re-read keeps the row the cursor is on.
#[gpui::test]
fn gh265_escaped_rename_keeps_the_cursor_row_for_a_held_down(cx: &mut TestAppContext) {
    let (shell, cx, mut lines, mut all) = hundred_k(cx);
    set_cursor(&shell, cx, 2);
    key(cx, "f2");
    cx.simulate_input("draft");
    key(cx, "enter");
    assert_eq!(sent(&mut lines).len(), 1, "the rename is out");
    key(cx, "up");
    key(cx, "escape");
    key(cx, "up");
    assert_eq!(
        cursor(&shell, cx),
        1,
        "Up after Escape moves off the renamed row"
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
        2,
        "the held Down moves one row from the row the re-read keeps"
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

// A write that lands with no destination of its own (rename-kept) still keeps a cursor Escape and Up moved off the row.
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
    key(cx, "escape");
    key(cx, "up");
    assert_eq!(
        cursor(&shell, cx),
        1,
        "Up after Escape moves off the renamed row"
    );
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
