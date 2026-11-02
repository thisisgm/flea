//! EY-62: after Flea's own trash the cursor's row takes the selection, as v0.3.12's Anchor.afterDelete lands on the row that takes the place.
use super::fileops_tests::{reply, wire};
use super::key_rows_tests::{launched, state};
use super::menus_tests::settle;
use super::test_shell::{land, row, set_cursor};
use crate::backend::Backend;
use crate::window::navigate::Step;
use gpui::TestAppContext;

// The rows after the trash: notes.txt is gone from alpha, beta, notes.txt, todo.md.
fn without_notes() -> Vec<flea::protocol::RowFacts> {
    vec![row("alpha", true), row("beta", true), row("todo.md", false)]
}

#[gpui::test]
fn a_trash_selects_the_row_that_takes_its_place(cx: &mut TestAppContext) {
    let (shell, cx) = launched(cx, "list");
    let mut sent = wire(&shell, cx);
    cx.simulate_keystrokes("delete");
    settle(cx);
    assert_eq!(
        sent(),
        [r#"{"c":"trash","rows":[2],"menuId":0,"listing":1}"#]
    );
    reply(&shell, cx, r#"{"t":"trashed","ok":1,"failed":0}"#);
    land(&shell, cx, "/files", without_notes(), 2);
    assert_eq!(
        state(&shell, cx),
        (2, vec![2]),
        "todo.md takes the place and is selected"
    );
    // The second Delete trashes the row that took the place.
    cx.simulate_keystrokes("delete");
    settle(cx);
    assert_eq!(
        sent(),
        [r#"{"c":"trash","rows":[2],"menuId":0,"listing":2}"#]
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn a_trash_of_the_last_row_selects_the_row_above(cx: &mut TestAppContext) {
    let (shell, cx) = launched(cx, "list");
    let mut sent = wire(&shell, cx);
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().selected = Vec::new()
    });
    set_cursor(&shell, cx, 3);
    cx.simulate_keystrokes("delete");
    settle(cx);
    assert_eq!(
        sent(),
        [r#"{"c":"trash","rows":[3],"menuId":0,"listing":1}"#]
    );
    reply(&shell, cx, r#"{"t":"trashed","ok":1,"failed":0}"#);
    let left = vec![
        row("alpha", true),
        row("beta", true),
        row("notes.txt", false),
    ];
    land(&shell, cx, "/files", left, 2);
    assert_eq!(
        state(&shell, cx),
        (2, vec![2]),
        "the row above takes the selection"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn a_multi_row_trash_selects_the_row_landing_on_the_first_trashed_index(cx: &mut TestAppContext) {
    let (shell, cx) = launched(cx, "list");
    let mut sent = wire(&shell, cx);
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().selected = vec![0, 2]
    });
    cx.simulate_keystrokes("delete");
    settle(cx);
    assert_eq!(
        sent(),
        [r#"{"c":"trash","rows":[0,2],"menuId":0,"listing":1}"#]
    );
    reply(&shell, cx, r#"{"t":"trashed","ok":2,"failed":0}"#);
    land(
        &shell,
        cx,
        "/files",
        vec![row("beta", true), row("todo.md", false)],
        2,
    );
    assert_eq!(
        state(&shell, cx),
        (0, vec![0]),
        "beta lands on the first trashed index"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// Anchor.js landed=false: the cursor stays on its row, and the row is selected.
#[gpui::test]
fn a_failed_trash_keeps_the_cursor_row_and_selects_it(cx: &mut TestAppContext) {
    let (shell, cx) = launched(cx, "list");
    let mut sent = wire(&shell, cx);
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().selected = vec![0, 2]
    });
    cx.simulate_keystrokes("delete");
    settle(cx);
    assert_eq!(
        sent(),
        [r#"{"c":"trash","rows":[0,2],"menuId":0,"listing":1}"#]
    );
    reply(&shell, cx, r#"{"t":"trashed","ok":0,"failed":2}"#);
    let rows = vec![
        row("alpha", true),
        row("beta", true),
        row("notes.txt", false),
        row("todo.md", false),
    ];
    land(&shell, cx, "/files", rows, 2);
    assert_eq!(
        state(&shell, cx),
        (2, vec![2]),
        "notes.txt stays under the cursor and is selected"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// The re-read after a trash asks the backend for the folder again; wire() keeps only the trash lines.
#[gpui::test]
fn a_trash_reply_asks_the_backend_for_the_landing_listing(cx: &mut TestAppContext) {
    let (shell, cx) = launched(cx, "list");
    let (backend, mut read) = Backend::test_lines();
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().backend = Some(backend)
    });
    cx.simulate_keystrokes("delete");
    settle(cx);
    let before: Vec<String> = std::iter::from_fn(&mut read).collect();
    reply(&shell, cx, r#"{"t":"trashed","ok":1,"failed":0}"#);
    let lines: Vec<String> = std::iter::from_fn(&mut read).collect();
    assert!(
        !before
            .iter()
            .any(|line| flea::json::field_str(line, "c").as_deref() == Some("list")),
        "nothing lists the folder before the trash reply: {before:?}"
    );
    assert!(
        lines
            .iter()
            .any(|line| flea::json::field_str(line, "c").as_deref() == Some("list")),
        "the re-read lists the folder: {lines:?}"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// PaneWire.qml onTrashed returns before it clears anything when a reply names no rows.
#[gpui::test]
fn a_reply_naming_no_rows_keeps_the_marks(cx: &mut TestAppContext) {
    let (shell, cx) = launched(cx, "list");
    let _sent = wire(&shell, cx);
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().selected = vec![0, 2]
    });
    reply(&shell, cx, r#"{"t":"trashed","ok":0,"failed":0}"#);
    assert_eq!(
        state(&shell, cx).1,
        vec![0, 2],
        "the marks stay when nothing was named"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// Anchor.afterDelete moves the cursor only in the folder the trash was sent from.
#[gpui::test]
fn a_trash_reply_does_not_move_the_cursor_once_the_folder_changed(cx: &mut TestAppContext) {
    let (shell, cx) = launched(cx, "list");
    let mut sent = wire(&shell, cx);
    cx.simulate_keystrokes("delete");
    settle(cx);
    assert_eq!(
        sent(),
        [r#"{"c":"trash","rows":[2],"menuId":0,"listing":1}"#]
    );
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let _ = shell.step(0, Step::OpenPlace("/elsewhere".into()), window, cx);
        })
    });
    land(&shell, cx, "/elsewhere", without_notes(), 2);
    set_cursor(&shell, cx, 1);
    reply(&shell, cx, r#"{"t":"trashed","ok":1,"failed":0}"#);
    assert_eq!(
        state(&shell, cx),
        (1, Vec::new()),
        "row 2 of the trashed folder neither moves the cursor nor selects a row in this one"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
