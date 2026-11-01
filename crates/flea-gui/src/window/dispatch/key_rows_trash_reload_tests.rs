//! A reload started while Trash is pending must still select the replacement row.
use super::fileops_tests::{reply, wire};
use super::key_rows_tests::{launched, state};
use super::menus_tests::settle;
use super::test_shell::{asked, land, lists, row};
use crate::{backend::Backend, state::Shell};
use gpui::{Entity, TestAppContext, VisualTestContext};

fn without_notes() -> Vec<flea::protocol::RowFacts> {
    vec![row("alpha", true), row("beta", true), row("todo.md", false)]
}

fn pending_trash(
    cx: &mut TestAppContext,
) -> (
    Entity<Shell>,
    &mut VisualTestContext,
    impl FnMut() -> Vec<String>,
) {
    let (shell, cx) = launched(cx, "list");
    let mut sent = wire(&shell, cx);
    cx.simulate_keystrokes("delete");
    settle(cx);
    assert_eq!(
        sent(),
        [r#"{"c":"trash","rows":[2],"menuId":0,"listing":1}"#]
    );
    let before = lists(&shell, cx);
    cx.simulate_keystrokes("f5");
    settle(cx);
    assert_eq!(lists(&shell, cx), before + 1);
    assert!(shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().pane.busy()));
    (shell, cx, sent)
}

#[gpui::test]
fn a_trash_reply_during_reload_selects_the_replacement_row(cx: &mut TestAppContext) {
    let (shell, cx) = launched(cx, "list");
    let (backend, mut read) = Backend::test_lines();
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().backend = Some(backend)
    });
    cx.simulate_keystrokes("delete");
    settle(cx);
    let trash: Vec<_> = std::iter::from_fn(&mut read).collect();
    assert!(trash
        .iter()
        .any(|line| { line == r#"{"c":"trash","rows":[2],"menuId":0,"listing":1}"# }));
    cx.simulate_keystrokes("f5");
    settle(cx);
    let reload: Vec<_> = std::iter::from_fn(&mut read).collect();
    assert!(
        reload
            .iter()
            .any(|line| { flea::json::field_str(line, "c").as_deref() == Some("list") }),
        "F5 starts a real listing before the Trash reply: {reload:?}"
    );
    assert!(shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().pane.busy()
    }));
    reply(&shell, cx, r#"{"t":"trashed","ok":1,"failed":0}"#);
    land(&shell, cx, "/files", without_notes(), 2);
    if shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().pane.busy()) {
        land(&shell, cx, "/files", without_notes(), 3);
    }
    assert_eq!(
        state(&shell, cx),
        (2, vec![2]),
        "the overlapping reload must select todo.md after Trash"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn a_trash_reply_during_reload_rereads_a_stale_refresh(cx: &mut TestAppContext) {
    let (shell, cx, _sent) = pending_trash(cx);
    reply(&shell, cx, r#"{"t":"trashed","ok":1,"failed":0}"#);
    let before = lists(&shell, cx);
    land(&shell, cx, "/files", super::menus_tests::home(), 2);
    assert_eq!(
        lists(&shell, cx),
        before + 1,
        "the refresh may predate Trash"
    );
    land(&shell, cx, "/files", without_notes(), 3);
    assert_eq!(state(&shell, cx), (2, vec![2]));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn a_failed_trash_during_reload_keeps_the_original_cursor_name(cx: &mut TestAppContext) {
    let (shell, cx, _sent) = pending_trash(cx);
    reply(&shell, cx, r#"{"t":"trashed","ok":0,"failed":1}"#);
    let mut shifted = super::menus_tests::home();
    shifted.insert(0, row("aardvark", true));
    let before = lists(&shell, cx);
    land(&shell, cx, "/files", shifted.clone(), 2);
    assert_eq!(lists(&shell, cx), before + 1);
    land(&shell, cx, "/files", shifted, 3);
    assert_eq!(
        state(&shell, cx),
        (3, vec![3]),
        "notes.txt keeps the selection by name"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn a_deferred_trash_landing_drops_when_navigation_leaves_the_folder(cx: &mut TestAppContext) {
    let (shell, cx, _sent) = pending_trash(cx);
    reply(&shell, cx, r#"{"t":"trashed","ok":1,"failed":0}"#);
    cx.simulate_keystrokes("backspace");
    settle(cx);
    assert_eq!(asked(&shell, cx), "/");
    let before = lists(&shell, cx);
    land(&shell, cx, "/", vec![row("elsewhere", true)], 2);
    assert_eq!(
        lists(&shell, cx),
        before,
        "no Trash reread in the new folder"
    );
    cx.simulate_keystrokes("f5");
    settle(cx);
    let before = lists(&shell, cx);
    land(&shell, cx, "/", vec![row("elsewhere", true)], 3);
    assert_eq!(lists(&shell, cx), before, "the old landing stays cancelled");
    assert!(state(&shell, cx).1.is_empty());
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn a_deferred_trash_landing_drops_when_the_refresh_fails(cx: &mut TestAppContext) {
    let (shell, cx, _sent) = pending_trash(cx);
    reply(&shell, cx, r#"{"t":"trashed","ok":1,"failed":0}"#);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(
                0,
                crate::backend::Event::Refused {
                    operation: "scan".into(),
                    path: "/files".into(),
                    message: "Permission denied".into(),
                    mode: 0,
                },
                window,
                cx,
            );
        })
    });
    settle(cx);
    cx.simulate_keystrokes("f5");
    settle(cx);
    let before = lists(&shell, cx);
    land(&shell, cx, "/files", without_notes(), 2);
    assert_eq!(
        lists(&shell, cx),
        before,
        "the failed listing drops its owed reread"
    );
    assert!(
        state(&shell, cx).1.is_empty(),
        "no stale Trash selection on a later reload"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn a_deferred_trash_landing_leaves_an_empty_listing_unselected(cx: &mut TestAppContext) {
    let (shell, cx, _sent) = pending_trash(cx);
    reply(&shell, cx, r#"{"t":"trashed","ok":1,"failed":0}"#);
    let before = lists(&shell, cx);
    land(&shell, cx, "/files", super::menus_tests::home(), 2);
    assert_eq!(lists(&shell, cx), before + 1);
    land(&shell, cx, "/files", Vec::new(), 3);
    assert!(state(&shell, cx).1.is_empty());
    assert!(!shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().pane.busy()));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
