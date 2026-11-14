use super::{close, editing, ready, receive, sent, set_cursor};
use crate::backend::Event;
use crate::window::u07_tests::read;
use gpui::TestAppContext;

#[path = "rename_wording_tests.rs"]
mod wording;

fn row_identity(shell: &mut crate::state::Shell, source: Option<&str>) {
    let host = shell.panes[0].as_mut().unwrap();
    let held = &host.pane.held;
    let mut rows = held.rows.clone();
    rows[2].source = source.map(str::to_owned);
    host.pane.held = std::rc::Rc::new(crate::list::pane::Held {
        listing: held.listing,
        total: held.total,
        start: held.start,
        rows,
    });
}

#[gpui::test]
fn inline_rename_keeps_the_identity_of_the_row_that_opened_the_editor(cx: &mut TestAppContext) {
    let (shell, cx, mut lines) = ready(cx, "default");
    shell.update(cx, |shell, _| row_identity(shell, Some("7:77:32768")));
    cx.simulate_keystrokes("r");
    shell.update(cx, |shell, _| {
        row_identity(shell, Some("9:99:32768"));
    });
    cx.simulate_input("draft");
    cx.simulate_keystrokes("enter");
    let requests = sent(&mut lines);
    assert_eq!(requests.len(), 1);
    assert_eq!(
        flea::json::field_str(&requests[0], "source").as_deref(),
        Some("7:77:32768")
    );
    close(&shell, cx);
}

#[gpui::test]
fn a_row_without_identity_waits_for_the_snapshot_before_its_editor_opens(cx: &mut TestAppContext) {
    let (shell, cx, mut lines) = ready(cx, "default");
    shell.update(cx, |shell, _| row_identity(shell, None));
    cx.simulate_keystrokes("f2");
    assert!(editing(&shell, cx).is_none());
    cx.simulate_keystrokes("enter");
    let requests = sent(&mut lines);
    assert_eq!(requests.len(), 1);
    assert_eq!(
        flea::json::field_str(&requests[0], "op").as_deref(),
        Some("snapshot")
    );
    let id = flea::json::field_usize(&requests[0], "id").unwrap();
    receive(
        &shell,
        cx,
        Event::Operations(format!(
            r#"{{"t":"menuaction","id":{id},"op":"snapshot","ok":true}}"#
        )),
    );
    assert!(editing(&shell, cx).is_some());
    cx.simulate_input("draft");
    cx.simulate_keystrokes("enter");
    let requests = sent(&mut lines);
    assert_eq!(flea::json::field_usize(&requests[0], "menuId"), Some(id));
    receive(
        &shell,
        cx,
        Event::Refused {
            operation: "rename".into(),
            path: "/files/notes.txt".into(),
            message: "Selected item changed; reopen the menu.".into(),
            mode: 0,
        },
    );
    let error = shell.read_with(cx, |shell, _| {
        shell.rename.edit.as_ref().unwrap().error.clone()
    });
    assert_eq!(error, "Item changed; reopen Rename.");
    cx.simulate_keystrokes("escape");
    assert!(sent(&mut lines)
        .iter()
        .any(|line| flea::json::field_str(line, "op").as_deref() == Some("close")));
    close(&shell, cx);
}

#[gpui::test]
fn a_cancelled_identity_snapshot_cannot_open_a_late_editor(cx: &mut TestAppContext) {
    let (shell, cx, mut lines) = ready(cx, "default");
    shell.update(cx, |shell, _| row_identity(shell, None));
    cx.simulate_keystrokes("f2");
    let requests = sent(&mut lines);
    let id = flea::json::field_usize(&requests[0], "id").unwrap();
    cx.simulate_keystrokes("escape");
    receive(
        &shell,
        cx,
        Event::Operations(format!(
            r#"{{"t":"menuaction","id":{id},"op":"snapshot","ok":true}}"#
        )),
    );
    assert!(editing(&shell, cx).is_none());
    assert!(sent(&mut lines)
        .iter()
        .any(|line| flea::json::field_str(line, "op").as_deref() == Some("close")));
    close(&shell, cx);
}

// Opens F2 on the identity-less cursor row and returns the snapshot id it asked for.
fn snapshot_pending(
    shell: &gpui::Entity<crate::state::Shell>,
    cx: &mut gpui::VisualTestContext,
    lines: &mut super::Lines,
) -> usize {
    shell.update(cx, |shell, _| row_identity(shell, None));
    cx.simulate_keystrokes("f2");
    let requests = sent(lines);
    assert_eq!(
        flea::json::field_str(&requests[0], "op").as_deref(),
        Some("snapshot")
    );
    flea::json::field_usize(&requests[0], "id").unwrap()
}

fn closed(lines: &mut super::Lines, id: usize) -> bool {
    sent(lines).iter().any(|line| {
        flea::json::field_str(line, "op").as_deref() == Some("close")
            && flea::json::field_usize(line, "id") == Some(id)
    })
}

#[gpui::test]
fn a_refused_identity_snapshot_says_why_and_opens_no_editor(cx: &mut TestAppContext) {
    let (shell, cx, mut lines) = ready(cx, "default");
    let id = snapshot_pending(&shell, cx, &mut lines);
    receive(
        &shell,
        cx,
        Event::Operations(format!(
            r#"{{"t":"menuaction","id":{id},"op":"snapshot","ok":false,"error":"Selected items changed; reopen the menu."}}"#
        )),
    );
    assert!(editing(&shell, cx).is_none());
    assert_eq!(
        read(&shell, cx, "lastMessage", &[]),
        "Item changed; reopen Rename."
    );
    assert!(closed(&mut lines, id));
    close(&shell, cx);
}

#[gpui::test]
fn a_snapshot_answered_after_the_cursor_moved_opens_no_editor(cx: &mut TestAppContext) {
    let (shell, cx, mut lines) = ready(cx, "default");
    let id = snapshot_pending(&shell, cx, &mut lines);
    set_cursor(&shell, cx, 1);
    receive(
        &shell,
        cx,
        Event::Operations(format!(
            r#"{{"t":"menuaction","id":{id},"op":"snapshot","ok":true}}"#
        )),
    );
    assert!(editing(&shell, cx).is_none());
    assert_eq!(read(&shell, cx, "lastMessage", &[]), "");
    assert!(closed(&mut lines, id));
    close(&shell, cx);
}

#[gpui::test]
fn a_drained_backend_cancels_the_identity_snapshot(cx: &mut TestAppContext) {
    let (shell, cx, mut lines) = ready(cx, "default");
    let id = snapshot_pending(&shell, cx, &mut lines);
    receive(&shell, cx, Event::Drained);
    assert!(shell.read_with(cx, |shell, _| !shell.panes[0]
        .as_ref()
        .unwrap()
        .nav
        .gate
        .rename_pending));
    receive(
        &shell,
        cx,
        Event::Operations(format!(
            r#"{{"t":"menuaction","id":{id},"op":"snapshot","ok":true}}"#
        )),
    );
    assert!(editing(&shell, cx).is_none());
    close(&shell, cx);
}
