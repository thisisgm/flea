//! Tab completion publishes its sentences through the mounted footer's timed message service.
use super::*;
use crate::path::view::PathView;
use gpui::{Entity, VisualTestContext};
use std::time::Duration;

fn start(cx: &mut TestAppContext, text: &str) -> (Handle, Entity<PathView>) {
    let handle = open(cx);
    let path = edit(cx, handle, |shell, window, cx| {
        shell.chrome_action("pathBar", window, cx);
        shell
            .chrome
            .view
            .as_ref()
            .unwrap()
            .read_with(cx, |chrome, _| chrome.path.clone())
    });
    frame(cx, handle);
    VisualTestContext::from_window(handle.into(), cx).simulate_input(text);
    frame(cx, handle);
    assert_eq!(path.read_with(cx, |path, _| path.entry.text.clone()), text);
    tab(cx, handle);
    path.read_with(cx, |path, _| {
        let pending = path.entry.pending().expect("Tab requested a backend peek");
        assert_eq!(pending.dir, "/files");
        assert!(!pending.hidden);
    });
    (handle, path)
}

fn tab(cx: &mut TestAppContext, handle: Handle) {
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("tab");
    frame(cx, handle);
}

fn answer(cx: &mut TestAppContext, handle: Handle, rows: &[(&str, bool)]) {
    edit(cx, handle, |shell, window, cx| {
        let document = crate::window::columns_tests::host::document(shell, "/files", rows, false);
        shell.receive(
            0,
            Backend::test_reply_line(&flea::jsondoc::render(&document)),
            window,
            cx,
        );
    });
    frame(cx, handle);
}

fn check_message(cx: &mut TestAppContext, handle: Handle, expected: &str) {
    assert_eq!(call(cx, handle, "lastMessage"), Reply::value(expected));
    assert_eq!(call(cx, handle, "statusPrimary"), Reply::value(expected));
    assert_eq!(call(cx, handle, "statusError"), Reply::value("false"));
    edit(cx, handle, |shell, _, cx| {
        assert!(shell.slots.status.is_some(), "the real footer is mounted");
        let footer = shell.surfaces[0].footer.as_ref().unwrap().read(cx);
        let line = footer.line.read(cx);
        assert_eq!(line.messages().notice(), expected);
        if !expected.is_empty() {
            let painted = line.painted.get("primary").expect("a painted sentence");
            assert_eq!(painted.text.as_ref(), expected);
            assert!(f32::from(painted.bounds.size.width) > 0.);
            assert!(f32::from(painted.bounds.size.height) > 0.);
        }
    });
}

fn expire(cx: &mut TestAppContext, handle: Handle) {
    cx.executor().advance_clock(Duration::from_millis(
        flea::ui_logic_status_feedback::messages::MESSAGE_MS + 1,
    ));
    cx.run_until_parked();
    frame(cx, handle);
    check_message(cx, handle, "");
}

#[gpui::test]
fn fix_pathtab_many_matches_reach_the_footer_and_expire(cx: &mut TestAppContext) {
    let (handle, path) = start(cx, "/files/doc");
    answer(
        cx,
        handle,
        &[("documents", true), ("docs", true), ("doc.txt", false)],
    );
    assert_eq!(
        path.read_with(cx, |path, _| path.entry.text.clone()),
        "/files/doc"
    );
    check_message(cx, handle, "2 items start with that");
    expire(cx, handle);
    tab(cx, handle);
    assert!(path.read_with(cx, |path, _| path.entry.pending().is_none()));
    check_message(cx, handle, "2 items start with that");
    expire(cx, handle);
}

#[gpui::test]
fn fix_pathtab_no_match_reaches_the_footer_and_expires(cx: &mut TestAppContext) {
    let (handle, path) = start(cx, "/files/missing");
    answer(cx, handle, &[("docs", true), ("missing.txt", false)]);
    assert_eq!(
        path.read_with(cx, |path, _| path.entry.text.clone()),
        "/files/missing"
    );
    check_message(cx, handle, "Nothing in /files starts with that");
    expire(cx, handle);
    tab(cx, handle);
    assert!(path.read_with(cx, |path, _| path.entry.pending().is_none()));
    check_message(cx, handle, "Nothing in /files starts with that");
    expire(cx, handle);
}

#[gpui::test]
fn fix_pathtab_unique_match_completes_silently(cx: &mut TestAppContext) {
    let (handle, path) = start(cx, "/files/doc");
    answer(cx, handle, &[("docs", true), ("doc.txt", false)]);
    assert_eq!(
        path.read_with(cx, |path, _| path.entry.text.clone()),
        "/files/docs/"
    );
    check_message(cx, handle, "");
}

#[gpui::test]
fn fix_pathtab_shared_prefix_grows_silently_until_cached_tab_cannot_extend_it(
    cx: &mut TestAppContext,
) {
    let (handle, path) = start(cx, "/files/d");
    answer(cx, handle, &[("documents", true), ("docs", true)]);
    assert_eq!(
        path.read_with(cx, |path, _| path.entry.text.clone()),
        "/files/doc"
    );
    check_message(cx, handle, "");
    tab(cx, handle);
    assert!(path.read_with(cx, |path, _| path.entry.pending().is_none()));
    check_message(cx, handle, "2 items start with that");
}
