//! EY-65: a folder whose disk facts never come still answers "unknown" at the bar's right end.
use super::*;
use crate::backend::{status::Filesystem, Backend, Event as BackendEvent};

fn filesystem(cx: &mut TestAppContext, handle: WindowHandle<Shell>) -> String {
    json(cx, handle, "statusFooterState")
        .get("filesystem")
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}

fn refuse_scan(cx: &mut TestAppContext, handle: WindowHandle<Shell>) {
    handle
        .update(cx, |shell, window, cx| {
            let pane = &mut shell.panes[0].as_mut().unwrap().pane;
            assert!(pane.begin("/files".into(), Default::default(), pane.now()));
            shell.receive(
                0,
                BackendEvent::Refused {
                    operation: "scan".into(),
                    path: "/files".into(),
                    message: "permission denied".into(),
                    mode: 0,
                },
                window,
                cx,
            );
        })
        .unwrap();
    frame(cx, handle);
}

#[gpui::test]
fn ey65_folder_answered_with_nothing_shows_unknown(cx: &mut TestAppContext) {
    let handle = open(cx);
    cx.simulate_window_resize(handle.into(), gpui::size(gpui::px(1100.), gpui::px(700.)));
    frame(cx, handle);
    assert_eq!(filesystem(cx, handle), "", "the answer is still out");
    handle
        .update(cx, |shell, window, cx| {
            // The backend's nothing answer on the wire: no name, and -1 for no free figure.
            let event = Backend::test_reply_line(
                r#"{"t":"fsinfo","path":"/files","fs":"","free":-1,"class":""}"#,
            );
            shell.receive(0, event, window, cx);
        })
        .unwrap();
    frame(cx, handle);
    assert_eq!(filesystem(cx, handle), "unknown");
}

#[gpui::test]
fn ey65_an_answer_for_the_unmoved_base_lands_on_the_shown_folder(cx: &mut TestAppContext) {
    let handle = open(cx);
    cx.simulate_window_resize(handle.into(), gpui::size(gpui::px(1100.), gpui::px(700.)));
    frame(cx, handle);
    assert_eq!(filesystem(cx, handle), "", "the answer is still out");
    handle
        .update(cx, |shell, window, cx| {
            let event = Backend::test_reply_line(
                r#"{"t":"fsinfo","path":"","fs":"btrfs","free":442000000000}"#,
            );
            shell.receive(0, event, window, cx);
        })
        .unwrap();
    frame(cx, handle);
    assert_eq!(filesystem(cx, handle), "btrfs · 442.0 GB free");
}

#[gpui::test]
fn ey65_locked_folder_shows_unknown_when_its_listing_is_refused(cx: &mut TestAppContext) {
    let (backend, mut requests) = Backend::test_lines();
    let handle = cx.add_window(|_, cx| crate::state::tests::shell_with(backend, cx));
    cx.simulate_window_resize(handle.into(), gpui::size(gpui::px(1100.), gpui::px(700.)));
    frame(cx, handle);
    while requests().is_some() {}
    refuse_scan(cx, handle);
    frame(cx, handle);
    let asked = std::iter::from_fn(&mut requests).any(|line| line.contains("\"fsinfo\""));
    assert!(
        asked,
        "settling a locked folder still asks for its disk facts"
    );
    assert_eq!(filesystem(cx, handle), "unknown");
}

// EY-465: a dead backend answers no fsinfo, so the disk fact stays empty (v0.3.12 fsAnswered is set only by fsinfo).
#[gpui::test]
fn ey465_failed_backend_leaves_the_unanswered_disk_fact_empty(cx: &mut TestAppContext) {
    let handle = open(cx);
    cx.simulate_window_resize(handle.into(), gpui::size(gpui::px(1100.), gpui::px(700.)));
    frame(cx, handle);
    handle
        .update(cx, |shell, window, cx| {
            shell.receive(
                0,
                BackendEvent::Failed(crate::backend::Failure {
                    key: "window.backend_stopped",
                    detail: String::new(),
                }),
                window,
                cx,
            );
        })
        .unwrap();
    frame(cx, handle);
    assert_eq!(read(cx, handle, "statusError"), Reply::value("true"));
    assert_eq!(filesystem(cx, handle), "");
}

#[gpui::test]
fn ey65_pending_answer_is_empty_then_known_facts_survive_an_error(cx: &mut TestAppContext) {
    let handle = open(cx);
    cx.simulate_window_resize(handle.into(), gpui::size(gpui::px(1100.), gpui::px(700.)));
    frame(cx, handle);
    assert_eq!(filesystem(cx, handle), "", "the answer is still out");
    handle
        .update(cx, |shell, window, cx| {
            shell.receive(
                0,
                BackendEvent::Filesystem(Filesystem {
                    class: String::new(),
                    tagable: None,
                    path: "/files".into(),
                    name: "btrfs".into(),
                    free: 442_000_000_000,
                }),
                window,
                cx,
            );
        })
        .unwrap();
    frame(cx, handle);
    assert_eq!(filesystem(cx, handle), "btrfs · 442.0 GB free");
    handle
        .update(cx, |shell, window, cx| {
            shell.receive(
                0,
                BackendEvent::Refused {
                    operation: "window".into(),
                    path: "/files".into(),
                    message: "permission denied".into(),
                    mode: 0,
                },
                window,
                cx,
            );
        })
        .unwrap();
    frame(cx, handle);
    assert_eq!(read(cx, handle, "statusError"), Reply::value("true"));
    assert_eq!(
        filesystem(cx, handle),
        "btrfs · 442.0 GB free",
        "an error on the left never evicts the disk fact on the right"
    );
}

#[gpui::test]
fn ey65_a_stale_fsinfo_refusal_leaves_the_shown_folder_pending(cx: &mut TestAppContext) {
    let handle = open(cx);
    cx.simulate_window_resize(handle.into(), gpui::size(gpui::px(1100.), gpui::px(700.)));
    frame(cx, handle);
    handle
        .update(cx, |shell, window, cx| {
            shell.receive(
                0,
                BackendEvent::Refused {
                    operation: "fsinfo".into(),
                    path: "/elsewhere".into(),
                    message: "gone".into(),
                    mode: 0,
                },
                window,
                cx,
            );
        })
        .unwrap();
    frame(cx, handle);
    assert_eq!(
        filesystem(cx, handle),
        "",
        "a refusal for another folder answers nothing here"
    );
}

#[gpui::test]
fn ey420_recent_spans_disks_so_the_bar_says_unknown(cx: &mut TestAppContext) {
    let handle = open(cx);
    cx.simulate_window_resize(handle.into(), gpui::size(gpui::px(1100.), gpui::px(700.)));
    frame(cx, handle);
    handle
        .update(cx, |shell, window, cx| {
            let facts = Filesystem {
                class: String::new(),
                tagable: None,
                path: "/files".into(),
                name: "tmpfs".into(),
                free: 3_100_000_000,
            };
            shell.receive(0, BackendEvent::Filesystem(facts), window, cx);
        })
        .unwrap();
    frame(cx, handle);
    assert_eq!(filesystem(cx, handle), "tmpfs · 3.1 GB free");
    handle
        .update(cx, |shell, _, cx| {
            shell.panes[0].as_mut().unwrap().recent.on = true;
            cx.notify();
        })
        .unwrap();
    frame(cx, handle);
    assert_eq!(
        filesystem(cx, handle),
        "unknown",
        "v0.3.12 RecentMode.js clears fsName and answers, so Recent never shows the folder it came from"
    );
}
