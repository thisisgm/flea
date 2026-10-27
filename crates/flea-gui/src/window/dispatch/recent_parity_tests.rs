use super::test_shell::{asked, land, open_root, row, sent, status};
use crate::{navigation::history::Entry, state::Shell};
use gpui::{point, px, Entity, Modifiers, MouseButton, NavigationDirection, TestAppContext, VisualTestContext};

fn recent(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    land(shell, cx, "/files", vec![row("note.txt", false)], 1);
    shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.recent.on = true;
        host.recent.from = "/files".into();
        host.nav.history.forward.push(Entry::new("/future", "", 0));
    });
    sent();
}

#[gpui::test]
fn pb_recent_hidden_toggle_refuses_without_changing_preferences(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    recent(&shell, cx);
    cx.simulate_keystrokes(".");
    assert_eq!(status(&shell, cx), "Recent shows every item it lists, hidden ones included");
    shell.read_with(cx, |shell, _| assert!(!shell.panes[0].as_ref().unwrap().nav.folders.borrow().hidden));
    assert!(sent().is_empty(), "the refusal sends no request");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_recent_new_folder_key_refuses_before_mkdir(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    recent(&shell, cx);
    cx.simulate_keystrokes("ctrl-shift-n");
    assert_eq!(status(&shell, cx), "Recent cannot hold a new folder. Open a folder first");
    assert!(!sent().iter().any(|line| line.contains("mkdir")));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_recent_forward_side_button_does_not_consume_forward_history(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    recent(&shell, cx);
    cx.simulate_mouse_down(point(px(500.), px(300.)), MouseButton::Navigate(NavigationDirection::Forward), Modifiers::none());
    assert_eq!(asked(&shell, cx), "/files");
    shell.read_with(cx, |shell, _| assert_eq!(shell.panes[0].as_ref().unwrap().nav.history.forward.len(), 1));
    assert!(sent().is_empty());
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_recent_sort_never_persists_the_synthetic_root_order(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    recent(&shell, cx);
    cx.simulate_keystrokes("s");
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.nav.order.key, "size");
        assert_eq!(host.nav.writer.borrow().sent, 0);
        assert_eq!(host.nav.folders.borrow().get("/files"), None);
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_recent_new_tab_drops_mode_and_saves_its_origin_with_original_sort(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    recent(&shell, cx);
    shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.recent.stashed = Some(("kind".into(), true));
        host.nav.order = crate::navigation::folder_state::Order { key: "mtime", desc: true };
    });
    cx.simulate_keystrokes("ctrl-t");
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(!host.recent.on);
        assert!(host.recent.from.is_empty());
        assert!(host.recent.stashed.is_none());
        assert_eq!(host.nav.order.key, "kind");
        assert!(host.nav.order.desc);
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
