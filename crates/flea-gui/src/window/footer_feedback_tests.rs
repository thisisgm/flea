//! Footer feedback follows actual key dispatch and backend delivery in a mounted window.
use super::*;
use gpui::VisualTestContext;
use std::time::Duration;

fn hit(cx: &mut TestAppContext, handle: Handle, key: &str) {
    edit(cx, handle, |shell, window, cx| {
        window.focus(&shell.focus, cx)
    });
    frame(cx, handle);
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes(key);
    frame(cx, handle);
}

fn advance(cx: &mut TestAppContext, handle: Handle, millis: u64) {
    cx.executor().advance_clock(Duration::from_millis(millis));
    cx.run_until_parked();
    frame(cx, handle);
}

fn receive(cx: &mut TestAppContext, handle: Handle, line: &str) {
    edit(cx, handle, |shell, window, cx| {
        shell.receive(0, Backend::test_reply_line(line), window, cx);
    });
    frame(cx, handle);
}

fn copied(cx: &mut TestAppContext, handle: Handle) {
    edit(cx, handle, |shell, window, cx| {
        shell.view = View::List;
        window.focus(&shell.focus, cx);
        assert!(!shell.panes[0].as_ref().unwrap().pane.busy());
    });
    frame(cx, handle);
    hit(cx, handle, "ctrl-c");
    receive(cx, handle, r#"{"t":"paths","paths":["/files/file-0.txt"]}"#);
    receive(
        cx,
        handle,
        r#"{"t":"clip","op":"set","ok":true,"token":"footer-copy"}"#,
    );
    receive(
        cx,
        handle,
        r#"{"t":"clip","op":"changed","clip":"copy","paths":["/files/file-0.txt"],"token":"footer-copy","skipped":0}"#,
    );
    assert_eq!(
        call(cx, handle, "statusPrimary"),
        Reply::value("Copied 1 item")
    );
}

fn secondary(cx: &mut TestAppContext, handle: Handle) -> String {
    json(cx, handle, "statusFooterState")
        .get("secondary")
        .and_then(|value| value.get("text"))
        .and_then(Json::as_str)
        .expect("the painted footer's secondary text")
        .to_owned()
}

fn undo(cx: &mut TestAppContext, handle: Handle) -> String {
    json(cx, handle, "statusFooterState")
        .get("undo")
        .and_then(|value| value.get("text"))
        .and_then(Json::as_str)
        .expect("the painted footer's undo text")
        .to_owned()
}

#[gpui::test]
fn footer_feedback_copy_expiry_clears_its_paste_hint(cx: &mut TestAppContext) {
    let handle = open(cx);
    let (backend, _received) = Backend::test_lines();
    edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_mut().unwrap().backend = Some(backend)
    });
    copied(cx, handle);
    assert_eq!(secondary(cx, handle), " · p pastes");
    advance(cx, handle, flea::ui_logic_fb1::messages::MESSAGE_MS);
    assert_eq!(call(cx, handle, "statusPrimary"), Reply::value(""));
    assert_eq!(secondary(cx, handle), "");
}

#[gpui::test]
fn footer_feedback_link_receipt_replaces_copy_and_expires_with_undo(cx: &mut TestAppContext) {
    let handle = open(cx);
    let (backend, _received) = Backend::test_lines();
    edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_mut().unwrap().backend = Some(backend)
    });
    copied(cx, handle);
    receive(
        cx,
        handle,
        r#"{"t":"linked","ok":1,"failed":0,"skipped":0,"note":""}"#,
    );
    assert_eq!(
        call(cx, handle, "statusPrimary"),
        Reply::value("Linked 1 item")
    );
    assert_eq!(
        call(cx, handle, "lastMessage"),
        Reply::value("Linked 1 item · z undoes")
    );
    assert_eq!(secondary(cx, handle), "");
    assert_eq!(undo(cx, handle), "z undoes");
    advance(cx, handle, flea::ui_logic_fb1::messages::MESSAGE_MS);
    assert_eq!(call(cx, handle, "lastMessage"), Reply::value(""));
    assert_eq!(call(cx, handle, "statusPrimary"), Reply::value(""));
    assert_eq!(secondary(cx, handle), "");
    assert_eq!(undo(cx, handle), "");
}

#[gpui::test]
fn footer_feedback_link_counts_and_errors_survive_the_notice_timer(cx: &mut TestAppContext) {
    for (line, expected, error, undo_text) in [
        (
            r#"{"t":"linked","ok":2,"failed":1,"skipped":3}"#,
            "Linked 2 items · 1 failed · 3 skipped",
            false,
            "z undoes",
        ),
        (
            r#"{"t":"linked","ok":0,"failed":1,"skipped":0}"#,
            "Linked 0 items · 1 failed",
            true,
            "",
        ),
        (
            r#"{"t":"linked","ok":1,"failed":0,"skipped":0,"note":"Could not remove the replaced link"}"#,
            "Linked 1 item · Could not remove the replaced link",
            true,
            "z undoes",
        ),
    ] {
        let handle = open(cx);
        let (backend, _received) = Backend::test_lines();
        edit(cx, handle, |shell, _, _| {
            shell.panes[0].as_mut().unwrap().backend = Some(backend)
        });
        receive(cx, handle, line);
        assert_eq!(call(cx, handle, "statusPrimary"), Reply::value(expected));
        assert_eq!(
            call(cx, handle, "statusError"),
            Reply::value(if error { "true" } else { "false" })
        );
        assert_eq!(undo(cx, handle), undo_text);
        advance(cx, handle, flea::ui_logic_fb1::messages::MESSAGE_MS);
        assert_eq!(
            call(cx, handle, "statusPrimary"),
            Reply::value(if error { expected } else { "" })
        );
    }
}

#[gpui::test]
fn footer_feedback_f4_explains_non_list_views_then_clears(cx: &mut TestAppContext) {
    for view in [View::Columns, View::Grid, View::Dual] {
        let handle = open(cx);
        edit(cx, handle, |shell, _, _| {
            shell.view = view;
        });
        frame(cx, handle);
        hit(cx, handle, "f4");
        assert_eq!(
            call(cx, handle, "statusPrimary"),
            Reply::value("Autofit needs the list view.")
        );
        assert_eq!(call(cx, handle, "statusError"), Reply::value("false"));
        advance(cx, handle, flea::ui_logic_fb1::messages::MESSAGE_MS - 1);
        assert_eq!(
            call(cx, handle, "statusPrimary"),
            Reply::value("Autofit needs the list view.")
        );
        advance(cx, handle, 1);
        assert_eq!(call(cx, handle, "statusPrimary"), Reply::value(""));
        assert_eq!(call(cx, handle, "lastMessage"), Reply::value(""));
    }
}
