//! Deferred Tags opening follows the mounted root through every menu preparation order.
use super::super::{
    menus_tests::{actions, deliver, home, settle, validated},
    test_shell::{land, open_root, open_root_with, sent, set_cursor},
};
use crate::{backend::Event, state::Shell, window::u07_tests::read};
use flea::jsondoc::{self, Json};
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::time::Duration;

const LABELS: &str = "Red|Orange|Yellow|Green|Blue|Purple|Gray";

fn begin(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> u64 {
    land(shell, cx, "/files", home(), 1);
    set_cursor(shell, cx, 2);
    sent();
    cx.simulate_keystrokes("T");
    let id = shell.read_with(cx, |shell, _| shell.menu.captured.as_ref().unwrap().id);
    deliver(
        shell,
        cx,
        &format!(r#"{{"t":"menuaction","id":{id},"op":"snapshot","ok":true,"kinds":[33188]}}"#),
    );
    id
}

fn applications(shell: &Entity<Shell>, cx: &mut VisualTestContext, id: u64) {
    deliver(
        shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","id":{id},"op":"applications","ok":true,"applications":[]}}"#
        ),
    );
}

fn states(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> String {
    let (id, listing) = shell.read_with(cx, |shell, _| shell.menu.tags.asked_for_test().unwrap());
    format!(r#"{{"t":"tagstates","id":{id},"listing":{listing},"applied":[],"mixed":[]}}"#)
}

fn pending(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    assert_eq!(read(shell, cx, "contextMenuVisible", &[]), "false");
    assert_eq!(read(shell, cx, "contextMenuSubmenuEntries", &[]), "");
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.menu.flyout),
        Some("tags")
    );
}

fn opened(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    let state = jsondoc::parse(&read(shell, cx, "menuState", &[])).unwrap();
    assert_eq!(state.get("opened"), Some(&Json::Bool(true)));
    assert_eq!(state.get("submenu"), Some(&Json::Bool(true)));
    assert_eq!(state.get("lone"), Some(&Json::Bool(false)));
    let cursor: usize = read(shell, cx, "contextMenuCursor", &[]).parse().unwrap();
    assert_eq!(actions(shell, cx)[cursor], "tags");
    assert_eq!(read(shell, cx, "contextMenuSubmenuEntries", &[]), LABELS);
    assert_eq!(shell.read_with(cx, |shell, _| shell.menu.flyout), None);
    shell.read_with(cx, |shell, _| {
        let capture = shell.menu.captured.as_ref().unwrap();
        let host = shell.panes[capture.side].as_ref().unwrap();
        assert!(capture.ready);
        assert_eq!(capture.rows, [2]);
        assert_eq!(capture.listing, host.pane.held.listing);
        assert_eq!(capture.identity, shell.menu_identity());
        assert_eq!(shell.menu.menu.borrow().identity, capture.identity);
    });
}

#[gpui::test]
fn tag_intent_survives_states_before_applications(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    let id = begin(&shell, cx);
    let tags = states(&shell, cx);
    deliver(&shell, cx, &tags);
    pending(&shell, cx);
    applications(&shell, cx, id);
    settle(cx);
    opened(&shell, cx);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn tag_intent_waits_for_states_after_applications(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    let id = begin(&shell, cx);
    applications(&shell, cx, id);
    pending(&shell, cx);
    let tags = states(&shell, cx);
    deliver(&shell, cx, &tags);
    settle(cx);
    opened(&shell, cx);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn tag_intent_survives_script_discovery(cx: &mut TestAppContext) {
    let (shell, cx) = open_root_with(
        cx,
        r#""keys":"default","view":"list","menu":{"hidden":[]}"#,
        None,
    );
    let id = begin(&shell, cx);
    assert!(sent().iter().any(|line| line.contains("__menuScripts")));
    let tags = states(&shell, cx);
    deliver(&shell, cx, &tags);
    pending(&shell, cx);
    deliver(
        &shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","id":{id},"op":"validate","action":"__menuScripts","ok":true,"paths":["/files/notes.txt"]}}"#
        ),
    );
    assert!(sent().iter().any(|line| line.contains(r#""c":"actions""#)));
    deliver(
        &shell,
        cx,
        &format!(r#"{{"t":"actions","id":{id},"actions":[]}}"#),
    );
    pending(&shell, cx);
    applications(&shell, cx, id);
    settle(cx);
    opened(&shell, cx);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn tag_intent_survives_loading_minimum_display(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    let id = begin(&shell, cx);
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(150));
    cx.run_until_parked();
    assert!(shell.read_with(cx, |shell, _| shell.menu.presentation.loading));
    applications(&shell, cx, id);
    let tags = states(&shell, cx);
    deliver(&shell, cx, &tags);
    pending(&shell, cx);
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        assert!(window.simulate_next_frame(cx) > 0);
    });
    cx.run_until_parked();
    pending(&shell, cx);
    cx.executor().advance_clock(Duration::from_millis(99));
    cx.run_until_parked();
    pending(&shell, cx);
    cx.executor().advance_clock(Duration::from_millis(1));
    cx.run_until_parked();
    opened(&shell, cx);
    assert!(!shell.read_with(cx, |shell, _| shell.menu.presentation.loading));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn tag_intent_retirement_rejects_late_preparation(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    let id = begin(&shell, cx);
    let tags = states(&shell, cx);
    deliver(&shell, cx, &tags);
    cx.simulate_keystrokes("escape");
    applications(&shell, cx, id);
    deliver(&shell, cx, &tags);
    settle(cx);
    assert_eq!(read(&shell, cx, "contextMenuVisible", &[]), "false");
    assert!(shell.read_with(cx, |shell, _| shell.menu.captured.is_none()
        && shell.menu.flyout.is_none()));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn tag_intent_replacement_rejects_old_side_ticket_listing_and_selection(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    let old = begin(&shell, cx);
    let old_tags = states(&shell, cx);
    cx.simulate_keystrokes("escape");
    set_cursor(&shell, cx, 3);
    cx.simulate_keystrokes("T");
    let id = shell.read_with(cx, |shell, _| shell.menu.captured.as_ref().unwrap().id);
    deliver(
        &shell,
        cx,
        &format!(r#"{{"t":"menuaction","id":{id},"op":"snapshot","ok":true,"kinds":[33188]}}"#),
    );
    let tags = states(&shell, cx);
    let ticket = shell.read_with(cx, |shell, _| shell.menu.tags.asked_for_test());
    applications(&shell, cx, old);
    deliver(&shell, cx, &old_tags);
    deliver(&shell, cx, &tags.replace("\"listing\":1", "\"listing\":2"));
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(1, Event::Producer(tags.clone()), window, cx)
        })
    });
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.menu.tags.asked_for_test()),
        ticket
    );
    assert!(shell.read_with(cx, |shell, _| shell.menu.tags_reading));
    pending(&shell, cx);
    applications(&shell, cx, id);
    deliver(&shell, cx, &tags);
    settle(cx);
    assert_eq!(read(&shell, cx, "contextMenuSubmenuEntries", &[]), LABELS);
    assert_eq!(
        shell.read_with(cx, |shell, _| shell
            .menu
            .captured
            .as_ref()
            .unwrap()
            .rows
            .clone()),
        [3]
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn tag_intent_changed_selection_cannot_open_from_delayed_replies(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    let id = begin(&shell, cx);
    let tags = states(&shell, cx);
    set_cursor(&shell, cx, 3);
    deliver(&shell, cx, &tags);
    applications(&shell, cx, id);
    settle(cx);
    assert_eq!(read(&shell, cx, "contextMenuVisible", &[]), "false");
    assert!(shell.read_with(cx, |shell, _| shell.menu.captured.is_none()
        && shell.menu.flyout.is_none()));
    assert!(!sent().iter().any(|line| line.contains("settags")));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn tag_intent_duplicate_replies_do_not_replay_or_duplicate_digits(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    let id = begin(&shell, cx);
    let tags = states(&shell, cx);
    deliver(&shell, cx, &tags);
    applications(&shell, cx, id);
    settle(cx);
    opened(&shell, cx);
    sent();
    cx.simulate_keystrokes("1");
    cx.simulate_keystrokes("2");
    let requests = sent();
    assert_eq!(
        requests
            .iter()
            .filter(|line| line.contains("\"op\":\"validate\""))
            .count(),
        1
    );
    assert!(!requests.iter().any(|line| line.contains("settags")));
    validated(&shell, cx, "tags:tag1", &["/files/notes.txt"]);
    validated(&shell, cx, "tags:tag2", &["/files/notes.txt"]);
    let writes = sent()
        .into_iter()
        .filter_map(|line| jsondoc::parse(&line).ok())
        .filter(|line| line.get("c").and_then(Json::as_str) == Some("settags"))
        .collect::<Vec<_>>();
    assert_eq!(writes.len(), 2);
    for (write, name) in writes.iter().zip(["Red", "Orange"]) {
        assert_eq!(write.get("tag").and_then(Json::as_str), Some(name));
        assert_eq!(
            write.get("rows"),
            Some(&Json::Arr(vec![Json::Num("2".into())]))
        );
        assert_eq!(write.get("listing"), Some(&Json::Num("1".into())));
    }
    cx.simulate_keystrokes("escape");
    assert_eq!(read(&shell, cx, "contextMenuSubmenuEntries", &[]), "");
    deliver(&shell, cx, &tags);
    applications(&shell, cx, id);
    settle(cx);
    assert_eq!(read(&shell, cx, "contextMenuSubmenuEntries", &[]), "");
    assert_eq!(shell.read_with(cx, |shell, _| shell.menu.flyout), None);
    assert!(!sent()
        .iter()
        .any(|line| line.contains("settags") || line.contains("\"op\":\"validate\"")));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
