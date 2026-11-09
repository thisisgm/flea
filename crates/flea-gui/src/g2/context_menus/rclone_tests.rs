use super::join_tests::{read, ready, receive, Fixture};
use crate::g2::file_ops::model::Dialog;
use crate::{backend::Backend, state::tests::shell_with, window::root::Root};
use gpui::{AppContext, TestAppContext, VisualTestContext};
use std::{cell::RefCell, rc::Rc};

type Log = Rc<RefCell<Vec<String>>>;

// join_tests::open, keeping every backend line so the rclone and jobs requests can be read as well.
fn open_logged(cx: &mut TestAppContext) -> (Fixture, Log) {
    let (backend, mut receive) = Backend::test_lines();
    let log = Log::default();
    let seen = log.clone();
    let requests = move || loop {
        let line = receive()?;
        seen.borrow_mut().push(line.clone());
        let document = flea::jsondoc::parse(&line).unwrap();
        if matches!(
            document.get("c").and_then(flea::jsondoc::Json::as_str),
            Some("menuaction" | "actions")
        ) {
            return Some(line);
        }
    };
    let handle = cx.add_window(|window, cx| {
        let shell = cx.new(|cx| shell_with(backend, cx));
        shell.update(cx, |shell, cx| window.focus(&shell.focus, cx));
        Root { shell }
    });
    let fixture = Fixture {
        handle,
        requests: Box::new(requests),
    };
    (fixture, log)
}

fn sent(fixture: &mut Fixture, log: &Log) -> Vec<String> {
    while (fixture.requests)().is_some() {}
    log.borrow().clone()
}

// The live menu's Send with rclone > gdrive on three files, up to the mounted Send To card.
fn send_card(cx: &mut TestAppContext) -> (Fixture, Log) {
    let (mut fixture, log) = open_logged(cx);
    let handle = fixture.handle;
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("m");
    let id = ready(cx, &mut fixture);
    receive(
        cx,
        handle,
        r#"{"t":"rcloneremotes","id":1,"visible":true,"remotes":["gdrive"]}"#,
    );
    handle
        .update(cx, |root, _, cx| {
            assert_eq!(
                root.shell.read(cx).menu.rclone[0].borrow().flyout().len(),
                1,
                "the production receive path must accept the current remotes reply"
            );
        })
        .unwrap();
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(200));
    cx.run_until_parked();
    let model = read(cx, handle, "contextMenuModel").stdout;
    assert!(
        model.contains("sendRclone"),
        "configured remote must appear: {model}"
    );
    assert!(model.contains("Send with rclone"), "{model}");
    let entries = flea::jsondoc::parse(&model).unwrap();
    let index = entries
        .as_array()
        .unwrap()
        .iter()
        .position(|row| {
            row.get("action").and_then(flea::jsondoc::Json::as_str) == Some("sendRclone")
        })
        .unwrap();
    for _ in 0..entries.as_array().unwrap().len() {
        let cursor: usize = read(cx, handle, "contextMenuCursor")
            .stdout
            .trim()
            .parse()
            .unwrap();
        if cursor == index {
            break;
        }
        VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes(if cursor < index {
            "down"
        } else {
            "up"
        });
    }
    assert_eq!(
        read(cx, handle, "contextMenuCursor")
            .stdout
            .trim()
            .parse::<usize>()
            .unwrap(),
        index
    );
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("right enter");
    let validate = (fixture.requests)().expect("the remote validates the captured selection");
    assert!(validate.contains("sendRclone:gdrive"), "{validate}");
    receive(
        cx,
        handle,
        &format!(
            r#"{{"t":"menuaction","id":{id},"op":"validate","ok":true,"action":"sendRclone:gdrive","paths":["/files/a","/files/b","/files/c"]}}"#
        ),
    );
    (fixture, log)
}

// Types Uploads and presses Enter on the card, then Escape when asked, as keys reach the open card.
fn submit(cx: &mut TestAppContext, fixture: &Fixture, escape: bool) {
    fixture
        .handle
        .update(cx, |root, _, cx| {
            root.shell.update(cx, |shell, cx| {
                let view = shell.operation_view(0).expect("the send field is mounted");
                view.update(cx, |overlay, cx| {
                    if let Some(Dialog::SendTo { path, .. }) = &mut overlay.model.dialog {
                        *path = "Uploads".into();
                    }
                    overlay.model.key(&overlay.catalog, "enter", false);
                    if escape {
                        overlay.model.key(&overlay.catalog, "escape", false);
                    }
                    cx.notify();
                });
                shell.flush_operations(0, cx);
            });
        })
        .unwrap();
    cx.run_until_parked();
}

// The request id of the one rclone send the card asked for.
fn send_request(fixture: &mut Fixture, log: &Log) -> usize {
    let lines = sent(fixture, log);
    let sends: Vec<_> = lines
        .iter()
        .filter(|line| line.contains(r#""c":"rclone","op":"send""#))
        .collect();
    assert_eq!(sends.len(), 1, "{lines:?}");
    assert!(sends[0].contains(r#""folder":"Uploads""#), "{}", sends[0]);
    flea::json::field_usize(sends[0], "id").unwrap()
}

#[gpui::test]
fn vf6_configured_rclone_is_reachable_in_live_menu(cx: &mut TestAppContext) {
    let (fixture, _) = send_card(cx);
    let handle = fixture.handle;
    handle
        .update(cx, |root, _, cx| {
            let shell = root.shell.read(cx);
            let view = shell.operation_view(0).expect("the send field is mounted");
            let model = &view.read(cx).model;
            assert!(model
                .rclone_target
                .as_ref()
                .unwrap()
                .borrow()
                .card(&shell.locale.catalog)
                .unwrap()
                .title
                .contains("gdrive"));
            assert!(matches!(model.dialog, Some(Dialog::SendTo { .. })));
        })
        .unwrap();
    let focused = |cx: &mut TestAppContext| {
        let state = read(cx, handle, "keyDeliveryState").stdout;
        flea::jsondoc::parse(state.trim())
            .unwrap()
            .get("activeFocusItem")
            .and_then(flea::jsondoc::Json::as_str)
            .map(str::to_owned)
    };
    assert_eq!(focused(cx).as_deref(), Some("QQuickTextInput"));
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("tab");
    cx.run_until_parked();
    assert_eq!(focused(cx).as_deref(), Some("DialogButton"));
}

#[gpui::test]
fn vf6_escape_after_send_cancels_the_started_job_through_the_window(cx: &mut TestAppContext) {
    let (mut fixture, log) = send_card(cx);
    submit(cx, &fixture, true);
    let request = send_request(&mut fixture, &log);
    receive(
        cx,
        fixture.handle,
        &format!(r#"{{"t":"rclonestarted","requestId":{request},"id":19,"ok":true,"error":""}}"#),
    );
    cx.run_until_parked();
    let cancels = sent(&mut fixture, &log)
        .into_iter()
        .filter(|line| line == r#"{"c":"jobs","op":"cancel","id":19}"#)
        .count();
    assert_eq!(
        cancels, 1,
        "the closed card's send is cancelled once it starts"
    );
}

#[gpui::test]
fn vf6_started_send_without_escape_runs_on(cx: &mut TestAppContext) {
    let (mut fixture, log) = send_card(cx);
    submit(cx, &fixture, false);
    let request = send_request(&mut fixture, &log);
    receive(
        cx,
        fixture.handle,
        &format!(r#"{{"t":"rclonestarted","requestId":{request},"id":19,"ok":true,"error":""}}"#),
    );
    cx.run_until_parked();
    assert!(sent(&mut fixture, &log)
        .iter()
        .all(|line| !line.contains(r#""op":"cancel""#)));
    fixture
        .handle
        .update(cx, |root, _, cx| {
            let shell = root.shell.read(cx);
            let view = shell.operation_view(0).expect("the operations host stays");
            assert!(
                view.read(cx).model.dialog.is_none(),
                "the card closes when the backend takes the send"
            );
        })
        .unwrap();
}
