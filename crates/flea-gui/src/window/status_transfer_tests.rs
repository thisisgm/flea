//! Held completion replies retain the real transfer card, cancellation and journal availability.
use super::{draw, state};
use crate::{
    backend::{Backend, Event},
    window::root::Root,
};
use gpui::{AppContext, TestAppContext, VisualTestContext, WindowHandle};
use std::time::Duration;

#[path = "status_transfer_notice_tests.rs"]
mod notice;

fn line(cx: &mut TestAppContext, handle: WindowHandle<Root>, text: &str) {
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| {
                shell.receive(0, Event::Operations(text.into()), window, cx);
            })
        })
        .unwrap();
}

fn settle(cx: &mut TestAppContext, handle: WindowHandle<Root>) {
    cx.run_until_parked();
    cx.executor()
        .advance_clock(Duration::from_millis(crate::pane::swap::HOLD_MS));
    cx.run_until_parked();
    draw(cx, handle);
}

fn start(
    cx: &mut TestAppContext,
    count: usize,
) -> (WindowHandle<Root>, impl FnMut() -> Option<String>) {
    let (backend, received) = Backend::test_lines();
    let handle = cx.add_window(|window, cx| {
        let shell = cx.new(|cx| crate::state::tests::shell_with(backend, cx));
        let focus = shell.read(cx).focus.clone();
        window.focus(&focus, cx);
        Root { shell }
    });
    draw(cx, handle);
    line(
        cx,
        handle,
        &format!(r#"{{"t":"transferstarted","id":12,"n":{count},"moving":false}}"#),
    );
    line(
        cx,
        handle,
        &format!(
            r#"{{"t":"job","job":{{"id":12,"kind":"copy","head":"Copying 1 of {count}","file":"notes.txt","done":0,"total":{count},"state":"running","undo":false}}}}"#
        ),
    );
    settle(cx, handle);
    (handle, received)
}

#[gpui::test]
fn ey477_held_completion_keeps_cancel_and_undo_tracks_the_journal(cx: &mut TestAppContext) {
    let (handle, mut received) = start(cx, 1);
    line(
        cx,
        handle,
        r#"{"t":"job","job":{"id":12,"kind":"copy","head":"Copied 1 item","file":"notes.txt","done":1,"total":1,"state":"done","undo":true}}"#,
    );
    settle(cx, handle);
    let seen = state(cx, handle);
    assert_eq!(
        seen.get("transferCard")
            .unwrap()
            .get("visible")
            .unwrap()
            .as_bool(),
        Some(true),
        "transferdone is still held"
    );
    let activities = seen.get("activities").unwrap().as_array().unwrap();
    assert_eq!(activities.len(), 1);
    assert_eq!(seen.get("notice").unwrap().as_str(), Some(""));
    assert_eq!(activities[0].get("running").unwrap().as_bool(), Some(true));
    assert_eq!(seen.get("undoAvailable").unwrap().as_bool(), Some(false));
    while received().is_some() {}
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("escape");
    settle(cx, handle);
    assert_eq!(
        received().as_deref(),
        Some(r#"{"c":"transfercancel","id":12}"#)
    );
    let seen = state(cx, handle);
    assert_eq!(
        seen.get("activities").unwrap().as_array().unwrap()[0]
            .get("cancelling")
            .unwrap()
            .as_bool(),
        Some(true)
    );
    assert_eq!(
        seen.get("transferCard")
            .unwrap()
            .get("visible")
            .unwrap()
            .as_bool(),
        Some(true)
    );
    assert_eq!(
        seen.get("transferCard")
            .unwrap()
            .get("cancel")
            .unwrap()
            .get("enabled")
            .unwrap()
            .as_bool(),
        Some(false)
    );
    assert!(
        VisualTestContext::from_window(handle.into(), cx)
            .debug_bounds("op1-job-12-cancelling")
            .is_some(),
        "the card paints Cancelling in place of Cancel"
    );
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("escape");
    settle(cx, handle);
    assert!(received().is_none(), "cancellation is sent once");
    line(
        cx,
        handle,
        r#"{"t":"transferdone","id":12,"ok":1,"failed":0,"cancelled":false}"#,
    );
    settle(cx, handle);
    let seen = state(cx, handle);
    assert!(seen
        .get("activities")
        .unwrap()
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(
        seen.get("transferCard")
            .unwrap()
            .get("visible")
            .unwrap()
            .as_bool(),
        Some(false)
    );
    assert_eq!(seen.get("undoAvailable").unwrap().as_bool(), Some(true));
    assert!(seen
        .get("notice")
        .unwrap()
        .as_str()
        .unwrap()
        .contains("z undoes"));
    cx.executor().advance_clock(Duration::from_millis(
        flea::ui_logic_status_feedback::messages::MESSAGE_MS + crate::pane::swap::HOLD_MS,
    ));
    cx.run_until_parked();
    draw(cx, handle);
    assert_eq!(
        state(cx, handle).get("undoAvailable").unwrap().as_bool(),
        Some(true),
        "notice expiry does not erase the journal"
    );
    handle
        .update(cx, |root, _, cx| {
            root.shell.update(cx, |shell, cx| {
                shell.status_messages(cx, |messages, now| {
                    messages.say(
                        "Copied 3 items",
                        false,
                        "",
                        flea::ui_logic_status_feedback::status::Hint::Paste,
                        now,
                    )
                });
            })
        })
        .unwrap();
    draw(cx, handle);
    assert_eq!(
        state(cx, handle).get("undoAvailable").unwrap().as_bool(),
        Some(true),
        "a clipboard notice does not erase the journal"
    );
    handle
        .update(cx, |root, _, cx| {
            root.shell.update(cx, |shell, cx| {
                shell.status_messages(cx, |messages, now| {
                    messages.say(
                        "Previous transfer failed",
                        true,
                        "",
                        flea::ui_logic_status_feedback::status::Hint::None,
                        now,
                    );
                });
            })
        })
        .unwrap();
    line(
        cx,
        handle,
        r#"{"t":"transferstarted","id":13,"n":1,"moving":false}"#,
    );
    settle(cx, handle);
    let next = state(cx, handle);
    assert_eq!(next.get("errors").unwrap().as_f64(), Some(1.));
    assert_eq!(next.get("notice").unwrap().as_str(), Some(""));
}

#[gpui::test]
fn ey477_a_completed_transfer_without_undo_never_advertises_it(cx: &mut TestAppContext) {
    let (handle, _received) = start(cx, 1);
    line(
        cx,
        handle,
        r#"{"t":"job","job":{"id":12,"kind":"copy","head":"Copied 1 item","done":1,"total":1,"state":"done","undo":false}}"#,
    );
    line(
        cx,
        handle,
        r#"{"t":"transferdone","id":12,"ok":1,"failed":0}"#,
    );
    settle(cx, handle);
    let seen = state(cx, handle);
    assert_eq!(seen.get("undoAvailable").unwrap().as_bool(), Some(false));
    assert_eq!(seen.get("notice").unwrap().as_str(), Some("Copied 1 item"));
}

#[gpui::test]
fn ey477_refusing_another_request_keeps_the_live_transfer(cx: &mut TestAppContext) {
    let (handle, _received) = start(cx, 1);
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| {
                shell.receive(
                    0,
                    Event::Refused {
                        operation: "transfer".into(),
                        path: String::new(),
                        message: "Permission denied".into(),
                        mode: 0,
                    },
                    window,
                    cx,
                );
            })
        })
        .unwrap();
    settle(cx, handle);
    let seen = state(cx, handle);
    let activities = seen.get("activities").unwrap().as_array().unwrap();
    assert_eq!(
        activities.len(),
        1,
        "an unkeyed refusal cannot retire the existing transfer"
    );
    assert_eq!(activities[0].get("running").unwrap().as_bool(), Some(true));
    assert_eq!(
        seen.get("transferCard")
            .unwrap()
            .get("cancel")
            .unwrap()
            .get("enabled")
            .unwrap()
            .as_bool(),
        Some(true)
    );
    assert_eq!(seen.get("undoAvailable").unwrap().as_bool(), Some(false));
}

#[gpui::test]
fn ey477_a_cancelled_completion_has_no_undo_advice(cx: &mut TestAppContext) {
    let (handle, _received) = start(cx, 1);
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("escape");
    settle(cx, handle);
    line(
        cx,
        handle,
        r#"{"t":"job","job":{"id":12,"kind":"copy","head":"Cancelled","done":0,"total":1,"state":"done","undo":false,"cancelled":true}}"#,
    );
    settle(cx, handle);
    assert_eq!(
        state(cx, handle)
            .get("transferCard")
            .unwrap()
            .get("cancelling")
            .unwrap()
            .as_bool(),
        Some(true)
    );
    line(
        cx,
        handle,
        r#"{"t":"transferdone","id":12,"ok":0,"failed":0,"cancelled":true}"#,
    );
    settle(cx, handle);
    let seen = state(cx, handle);
    assert_eq!(
        seen.get("transferCard")
            .unwrap()
            .get("visible")
            .unwrap()
            .as_bool(),
        Some(false)
    );
    assert_eq!(seen.get("undoAvailable").unwrap().as_bool(), Some(false));
    let notice = seen.get("notice").unwrap().as_str().unwrap();
    assert!(notice.to_lowercase().contains("cancel"));
    assert!(!notice.contains("undo"));
}

#[gpui::test]
fn ey477_partial_failure_preserves_the_existing_undo_receipt(cx: &mut TestAppContext) {
    let (handle, _received) = start(cx, 2);
    line(
        cx,
        handle,
        r#"{"t":"job","job":{"id":12,"kind":"copy","head":"Copy failed","done":1,"total":2,"state":"failed","undo":false}}"#,
    );
    line(
        cx,
        handle,
        r#"{"t":"transferdone","id":12,"ok":1,"failed":1}"#,
    );
    settle(cx, handle);
    let seen = state(cx, handle);
    assert!(
        seen.get("notice")
            .unwrap()
            .as_str()
            .unwrap()
            .ends_with("z undoes"),
        "a partial transfer still journals its successful writes for generic Undo"
    );
}

#[gpui::test]
fn ey477_a_dead_backend_cannot_keep_a_completed_transfers_undo(cx: &mut TestAppContext) {
    let (handle, _received) = start(cx, 1);
    line(
        cx,
        handle,
        r#"{"t":"job","job":{"id":12,"kind":"copy","head":"Copied 1 item","done":1,"total":1,"state":"done","undo":true}}"#,
    );
    line(
        cx,
        handle,
        r#"{"t":"transferdone","id":12,"ok":1,"failed":0}"#,
    );
    settle(cx, handle);
    assert_eq!(
        state(cx, handle).get("undoAvailable").unwrap().as_bool(),
        Some(true)
    );
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| {
                shell.receive(
                    0,
                    Event::Failed(crate::backend::Failure {
                        key: "window.backend_eof",
                        detail: String::new(),
                    }),
                    window,
                    cx,
                );
            })
        })
        .unwrap();
    settle(cx, handle);
    assert_eq!(
        state(cx, handle).get("undoAvailable").unwrap().as_bool(),
        Some(false)
    );
}
