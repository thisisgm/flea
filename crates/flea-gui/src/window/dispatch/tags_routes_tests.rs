//! The legacy Order::Menu adapter and validated file-menu leaves share one ordered owner.
use super::menus_tests::{home, settle, validated};
use super::tags_tests::{complete, request, status};
use super::test_shell::{land, open_root, sent, set_cursor};
use crate::{backend::Backend, g2::context_menus::join::Chosen, window::u07_tests::read};
use flea::jsondoc::Json;
use gpui::TestAppContext;
use std::{cell::Cell, rc::Rc};

#[gpui::test]
fn pb_tag_order_menu_and_validated_leaf_share_completion_order(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    set_cursor(&shell, cx, 2);
    cx.simulate_keystrokes("T");
    super::menus_tests::tags_answer(&shell, cx, &[0o100644]);
    let capture = shell.read_with(cx, |shell, _| shell.menu.captured.clone().unwrap());
    cx.simulate_keystrokes("escape");
    cx.simulate_keystrokes("escape");
    assert_eq!(read(&shell, cx, "contextMenuVisible", &[]), "false");
    sent();
    let chosen = Chosen {
        captured: capture,
        action: "tags".into(),
        leaf: "tag2".into(),
        paths: vec![],
        claimed: Rc::new(Cell::new(false)),
    };
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.dispatch(super::Order::Menu(chosen.clone()), window, cx);
            shell.dispatch(super::Order::Menu(chosen), window, cx);
        })
    });
    let changes = sent()
        .into_iter()
        .filter_map(|line| flea::jsondoc::parse(&line).ok())
        .filter(|line| line.get("c").and_then(Json::as_str) == Some("settags"))
        .collect::<Vec<_>>();
    assert_eq!(changes.len(), 1, "a Chosen is claimed once");
    assert_eq!(changes[0].get("tag").and_then(Json::as_str), Some("Orange"));
    cx.simulate_keystrokes("T");
    super::menus_tests::tags_answer(&shell, cx, &[0o100644]);
    cx.simulate_keystrokes("1");
    validated(&shell, cx, "tags:tag1", &["/files/notes.txt"]);
    assert_eq!(
        request("settags").get("tag").and_then(Json::as_str),
        Some("Red")
    );
    complete(&shell, cx, r#"{"t":"settagged","ok":1,"op":"add"}"#);
    assert_eq!(status(&shell, cx), "Tagged 1 item Orange \u{b7} z undoes");
    complete(&shell, cx, r#"{"t":"settagged","ok":1,"op":"remove"}"#);
    assert_eq!(
        status(&shell, cx),
        "Removed Red from 1 item \u{b7} z undoes"
    );
    assert_eq!(read(&shell, cx, "contextMenuVisible", &[]), "true");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_tag_escape_discards_queued_validations(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    set_cursor(&shell, cx, 2);
    cx.simulate_keystrokes("T");
    super::menus_tests::tags_answer(&shell, cx, &[0o100644]);
    let id = shell.read_with(cx, |shell, _| shell.menu.captured.as_ref().unwrap().id);
    cx.simulate_keystrokes("1");
    cx.simulate_keystrokes("2");
    cx.simulate_keystrokes("escape");
    cx.simulate_keystrokes("escape");
    assert_eq!(read(&shell, cx, "contextMenuVisible", &[]), "false");
    sent();
    super::menus_tests::reply(
        &shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","id":{id},"op":"validate","ok":true,"action":"tags:tag1","paths":["/files/notes.txt"]}}"#
        ),
    );
    assert!(!sent()
        .iter()
        .any(|line| line.contains("settags") || line.contains("validate")));
    assert_eq!(read(&shell, cx, "contextMenuVisible", &[]), "false");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_tag_transport_failure_forgets_pending_identity(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    set_cursor(&shell, cx, 2);
    cx.simulate_keystrokes("T");
    super::menus_tests::tags_answer(&shell, cx, &[0o100644]);
    cx.simulate_keystrokes("1");
    validated(&shell, cx, "tags:tag1", &["/files/notes.txt"]);
    request("settags");
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(0, crate::backend::Event::Drained, window, cx)
        })
    });
    settle(cx);
    let (backend, _lines) = Backend::test_lines();
    shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.backend = Some(backend);
        host.stopped = false;
    });
    super::menus_tests::reply(&shell, cx, r#"{"t":"settagged","ok":1,"op":"add"}"#);
    assert!(!status(&shell, cx).contains("Tagged"));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_tag_old_order_menu_does_not_release_a_new_capture_validation(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    set_cursor(&shell, cx, 2);
    cx.simulate_keystrokes("m");
    super::menus_tests::tags_answer(&shell, cx, &[0o100644]);
    let capture = shell.read_with(cx, |shell, _| shell.menu.captured.clone().unwrap());
    cx.simulate_keystrokes("escape");
    cx.simulate_keystrokes("m");
    super::menus_tests::tags_answer(&shell, cx, &[0o100644]);
    assert_ne!(
        shell.read_with(cx, |shell, _| shell.menu.captured.as_ref().unwrap().id),
        capture.id
    );
    sent();
    cx.simulate_keystrokes("1");
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.dispatch(
                super::Order::Menu(Chosen {
                    captured: capture,
                    action: "tags".into(),
                    leaf: "tag2".into(),
                    paths: vec![],
                    claimed: Rc::new(Cell::new(false)),
                }),
                window,
                cx,
            )
        })
    });
    cx.simulate_keystrokes("3");
    let requests = sent();
    assert_eq!(
        requests
            .iter()
            .filter(|line| line.contains("\"op\":\"validate\""))
            .count(),
        1,
        "an old choice must not release the new capture's validation"
    );
    assert!(requests
        .iter()
        .any(|line| line.contains("\"tag\":\"Orange\"")));
    validated(&shell, cx, "tags:tag1", &["/files/notes.txt"]);
    assert_eq!(
        request("settags").get("tag").and_then(Json::as_str),
        Some("Red")
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_tag_non_tag_choice_waits_for_the_outstanding_tag_validation(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    set_cursor(&shell, cx, 2);
    cx.simulate_keystrokes("m");
    super::menus_tests::tags_answer(&shell, cx, &[0o100644]);
    sent();
    cx.simulate_keystrokes("1");
    super::menus_tests::choose(&shell, cx, "rename");
    assert_eq!(
        sent()
            .iter()
            .filter(|line| line.contains("\"op\":\"validate\""))
            .count(),
        1
    );
    validated(&shell, cx, "tags:tag1", &["/files/notes.txt"]);
    let requests = sent()
        .into_iter()
        .filter_map(|line| flea::jsondoc::parse(&line).ok())
        .collect::<Vec<_>>();
    assert!(
        requests.iter().any(
            |line| line.get("c").and_then(Json::as_str) == Some("settags")
                && line.get("tag").and_then(Json::as_str) == Some("Red")
        ),
        "the tag choice is delivered first"
    );
    assert!(
        requests.iter().any(
            |line| line.get("op").and_then(Json::as_str) == Some("validate")
                && line.get("action").and_then(Json::as_str) == Some("rename")
        ),
        "Rename validates next"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_tag_join_replaced_capture_waits_for_its_own_tag_states(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    set_cursor(&shell, cx, 2);
    cx.simulate_keystrokes("T");
    super::menus_tests::answer(&shell, cx, &[0o100644]);
    let old = shell.read_with(cx, |shell, _| shell.menu.tags.asked_for_test().unwrap());
    assert_eq!(read(&shell, cx, "contextMenuVisible", &[]), "false");
    cx.simulate_keystrokes("escape");
    cx.simulate_keystrokes("T");
    super::menus_tests::answer(&shell, cx, &[0o100644]);
    let current = shell.read_with(cx, |shell, _| shell.menu.tags.asked_for_test().unwrap());
    assert_ne!(old.0, current.0);
    for (id, listing) in [old, (current.0, current.1 + 1)] {
        super::menus_tests::reply(
            &shell,
            cx,
            &format!(
                r#"{{"t":"tagstates","id":{id},"listing":{listing},"applied":["Blue"],"mixed":[]}}"#
            ),
        );
        assert_eq!(read(&shell, cx, "contextMenuVisible", &[]), "false");
    }
    super::menus_tests::reply(
        &shell,
        cx,
        &format!(
            r#"{{"t":"tagstates","id":{},"listing":{},"applied":["Red"],"mixed":[]}}"#,
            current.0, current.1
        ),
    );
    assert_eq!(read(&shell, cx, "contextMenuVisible", &[]), "true");
    assert!(read(&shell, cx, "contextMenuSubmenuEntries", &[]).ends_with("|-|Remove All Tags"));
    assert!(read(&shell, cx, "contextMenuModel", &[]).contains(r#""state":"applied""#));
    sent();
    cx.simulate_keystrokes("1");
    validated(&shell, cx, "tags:tag1", &["/files/notes.txt"]);
    let change = request("settags");
    assert_eq!(change.get("listing"), Some(&Json::Num("1".into())));
    assert_eq!(
        change.get("rows").and_then(Json::as_array),
        Some(&[Json::Num("2".into())][..])
    );
    assert_eq!(read(&shell, cx, "contextMenuVisible", &[]), "true");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
