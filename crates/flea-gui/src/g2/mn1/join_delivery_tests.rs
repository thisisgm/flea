//! Snapshot success and failure through the window's backend event and native input paths.
use super::{
    join::Chosen,
    join_tests::{open, read, ready, Fixture},
};
use crate::{
    backend::{Backend, Event},
    state::{tests::shell_with, Shell},
    window::root::Root,
};
use gpui::{AppContext, TestAppContext, VisualTestContext, Window, WindowHandle};
use std::{cell::RefCell, rc::Rc};

fn update<R>(
    cx: &mut TestAppContext,
    handle: WindowHandle<Root>,
    callback: impl FnOnce(&mut Shell, &mut Window, &mut gpui::Context<Shell>) -> R,
) -> R {
    handle
        .update(cx, |root, window, cx| {
            root.shell
                .update(cx, |shell, cx| callback(shell, window, cx))
        })
        .unwrap()
}

fn key(cx: &mut TestAppContext, handle: WindowHandle<Root>, value: &str) {
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes(value);
}

fn seek(cx: &mut TestAppContext, handle: WindowHandle<Root>, action: &str) {
    let reply = read(cx, handle, "contextMenuModel");
    let model = flea::jsondoc::parse(reply.stdout.trim()).unwrap();
    let index = model
        .as_array()
        .unwrap()
        .iter()
        .position(|row| row.get("action").and_then(flea::jsondoc::Json::as_str) == Some(action))
        .expect("an offered row");
    for _ in 0..model.as_array().unwrap().len() {
        let cursor: usize = read(cx, handle, "contextMenuCursor")
            .stdout
            .trim()
            .parse()
            .unwrap();
        if cursor == index {
            return;
        }
        key(cx, handle, if cursor < index { "down" } else { "up" });
    }
    panic!("the keyboard must reach {action}");
}

fn receive(cx: &mut TestAppContext, handle: WindowHandle<Root>, line: &str) {
    update(cx, handle, |shell, window, cx| {
        shell.receive(0, Event::Producer(line.into()), window, cx)
    });
}

fn snapshot(fixture: &mut Fixture) -> u64 {
    let line = (fixture.requests)().expect("the backend receives a snapshot");
    assert!(line.contains("\"op\":\"snapshot\""), "{line}");
    assert!(
        line.contains("\"listing\":2"),
        "the captured generation is sent: {line}"
    );
    flea::json::field_usize(&line, "id").unwrap() as u64
}

#[gpui::test]
fn u06_live_menu_waits_for_snapshot_and_routes_the_captured_choice_once(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    let handle = fixture.handle;
    let entity = handle.update(cx, |root, _, _| root.shell.clone()).unwrap();
    let seen = Rc::new(RefCell::new(Vec::<Chosen>::new()));
    let log = seen.clone();
    let _subscription = cx.update(|cx| {
        cx.subscribe(&entity, move |_, chosen: &Chosen, _| {
            assert!(chosen.claim());
            log.borrow_mut().push(chosen.clone());
        })
    });
    key(cx, handle, "m");
    assert_eq!(read(cx, handle, "contextMenuVisible").stdout, "false\n");
    key(cx, handle, "enter");
    assert!(
        seen.borrow().is_empty(),
        "no choice exists before the complete menu"
    );
    let id = ready(cx, &mut fixture);
    seek(cx, handle, "rename");
    key(cx, handle, "enter");
    let validate = (fixture.requests)().expect("a choice validates the original snapshot");
    assert!(
        validate.contains("\"op\":\"validate\"") && validate.contains("\"action\":\"rename\""),
        "{validate}"
    );
    let reply = format!("{{\"t\":\"menuaction\",\"id\":{id},\"op\":\"validate\",\"ok\":true,\"action\":\"rename\",\"paths\":[\"/files/a\"]}}");
    receive(cx, handle, &reply);
    receive(cx, handle, &reply);
    let seen = seen.borrow();
    assert_eq!(
        seen.len(),
        1,
        "a duplicate backend reply cannot deliver again"
    );
    assert_eq!(
        (
            seen[0].captured.side,
            seen[0].captured.listing,
            seen[0].captured.cursor
        ),
        (0, 2, 0)
    );
    assert_eq!(seen[0].captured.rows, [0]);
    assert_eq!(seen[0].captured.id, id);
    assert_eq!(seen[0].paths, ["/files/a"]);
    assert!(
        (fixture.requests)().is_none(),
        "a claimed snapshot belongs to its producer"
    );
}

#[gpui::test]
fn u06_live_menu_late_snapshot_and_changed_identity_cannot_fire(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    let handle = fixture.handle;
    key(cx, handle, "m");
    let id = snapshot(&mut fixture);
    key(cx, handle, "escape");
    let close = (fixture.requests)().expect("dismissal retires the snapshot");
    assert!(close.contains("\"op\":\"close\""));
    receive(
        cx,
        handle,
        &format!(
            "{{\"t\":\"menuaction\",\"id\":{id},\"op\":\"snapshot\",\"ok\":true,\"kinds\":[]}}"
        ),
    );
    assert_eq!(read(cx, handle, "contextMenuVisible").stdout, "false\n");
    assert!(
        (fixture.requests)().is_none(),
        "retired replies start no discovery"
    );
    key(cx, handle, "m");
    let next = snapshot(&mut fixture);
    update(cx, handle, |shell, _, _| {
        shell.panes[0].as_mut().unwrap().pane.cursor = 1
    });
    receive(
        cx,
        handle,
        &format!(
            "{{\"t\":\"menuaction\",\"id\":{next},\"op\":\"snapshot\",\"ok\":true,\"kinds\":[]}}"
        ),
    );
    assert_eq!(read(cx, handle, "contextMenuVisible").stdout, "false\n");
    let close = (fixture.requests)().expect("stale identity closes the backend snapshot");
    assert!(close.contains("\"op\":\"close\""));
    update(cx, handle, |shell, _, _| {
        assert!(!shell.panes[0].as_ref().unwrap().nav.gate.owned())
    });
}

#[gpui::test]
fn u06_live_menu_unclaimed_choice_is_refused_and_snapshot_retired(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    let handle = fixture.handle;
    key(cx, handle, "m");
    let id = ready(cx, &mut fixture);
    seek(cx, handle, "rename");
    key(cx, handle, "enter");
    assert!((fixture.requests)()
        .unwrap()
        .contains("\"op\":\"validate\""));
    receive(cx, handle, &format!("{{\"t\":\"menuaction\",\"id\":{id},\"op\":\"validate\",\"ok\":true,\"action\":\"rename\",\"paths\":[\"/files/a\"]}}"));
    assert!((fixture.requests)().unwrap().contains("\"op\":\"close\""));
    update(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(!host.nav.gate.owned());
        assert_eq!(
            host.status.as_ref(),
            shell.locale.catalog.tr("menu.refusal.unavailable")
        );
    });
}

#[gpui::test]
fn u06_live_menu_send_failure_and_backend_drain_release_the_gate(cx: &mut TestAppContext) {
    let (backend, receive) = Backend::test_channel();
    drop(receive);
    let broken = cx.add_window(|window, cx| {
        let shell = cx.new(|cx| shell_with(backend, cx));
        shell.update(cx, |shell, cx| window.focus(&shell.focus, cx));
        Root { shell }
    });
    key(cx, broken, "m");
    assert_eq!(read(cx, broken, "contextMenuVisible").stdout, "false\n");
    update(cx, broken, |shell, _, _| {
        assert!(!shell.panes[0].as_ref().unwrap().nav.gate.owned())
    });
    let fixture = open(cx);
    let handle = fixture.handle;
    key(cx, handle, "m");
    update(cx, handle, |shell, window, cx| {
        shell.receive(0, Event::Drained, window, cx)
    });
    assert_eq!(read(cx, handle, "contextMenuVisible").stdout, "false\n");
    update(cx, handle, |shell, _, _| {
        assert!(!shell.panes[0].as_ref().unwrap().nav.gate.owned())
    });
}

#[gpui::test]
fn u06_live_menu_keyboard_keeps_marks_outside_cursor_and_refuses_changed_availability(
    cx: &mut TestAppContext,
) {
    let mut fixture = open(cx);
    let handle = fixture.handle;
    update(cx, handle, |shell, _, _| {
        shell.panes[0].as_mut().unwrap().selected = vec![2, 4]
    });
    key(cx, handle, "m");
    let id = snapshot(&mut fixture);
    update(cx, handle, |shell, _, _| {
        assert_eq!(shell.menu.captured.as_ref().unwrap().rows, [2, 4])
    });
    receive(cx, handle, &format!("{{\"t\":\"menuaction\",\"id\":{id},\"op\":\"snapshot\",\"ok\":true,\"kinds\":[32768,32768]}}"));
    seek(cx, handle, "rename");
    key(cx, handle, "enter");
    assert!((fixture.requests)()
        .unwrap()
        .contains("\"op\":\"validate\""));
    update(cx, handle, |shell, _, _| {
        shell.launch.settings.data =
            flea::jsondoc::parse("{\"menu\":{\"hidden\":[\"rename\"]}}").unwrap()
    });
    receive(cx, handle, &format!("{{\"t\":\"menuaction\",\"id\":{id},\"op\":\"validate\",\"ok\":true,\"action\":\"rename\",\"paths\":[\"/files/c\",\"/files/e\"]}}"));
    let line = (fixture.requests)().expect("the unavailable action closes its snapshot");
    assert!(line.contains("\"op\":\"close\""), "{line}");
    assert!(
        (fixture.requests)().is_none(),
        "no validate or action was sent"
    );
}

#[gpui::test]
fn u06_live_menu_closing_rearms_a_watch_that_fired_under_the_menu(cx: &mut TestAppContext) {
    let fixture = open(cx);
    let handle = fixture.handle;
    key(cx, handle, "m");
    update(cx, handle, |shell, window, cx| {
        shell.receive(
            0,
            Event::Changed {
                path: "/files".into(),
            },
            window,
            cx,
        )
    });
    update(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.nav.watch.due = Some(host.pane.now());
    });
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(401));
    cx.run_until_parked();
    update(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(host.nav.watch.stale, "the menu holds the changed directory");
        assert!(host.nav.watch.due.is_none(), "the first watch timer fired");
        assert_eq!(host.nav.watch.rereads, 0);
    });
    key(cx, handle, "escape");
    update(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(
            host.nav.watch.due.is_some() && host.watch_task.is_some(),
            "the released gate schedules the owed reread"
        );
    });
}

#[gpui::test]
fn u06_live_menu_keeps_the_window_quit_binding(cx: &mut TestAppContext) {
    let fixture = open(cx);
    let handle = fixture.handle;
    key(cx, handle, "m");
    key(cx, handle, "ctrl-q");
    update(cx, handle, |shell, _, _| {
        assert_ne!(
            shell.lifecycle.phase(),
            crate::window::lifecycle::Phase::Open
        )
    });
}
