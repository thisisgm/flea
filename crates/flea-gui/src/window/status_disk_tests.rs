use super::*;
use crate::backend::{status::Filesystem, Event as BackendEvent};

fn answer(cx: &mut TestAppContext, handle: WindowHandle<Shell>, path: &str, name: &str) {
    handle
        .update(cx, |shell, window, cx| {
            shell.receive(
                0,
                BackendEvent::Filesystem(Filesystem {
                    class: String::new(),
                    tagable: None,
                    path: path.into(),
                    name: name.into(),
                    free: 442_000_000_000,
                }),
                window,
                cx,
            );
        })
        .unwrap();
    frame(cx, handle);
}

fn change_folder(cx: &mut TestAppContext, handle: WindowHandle<Shell>) {
    handle
        .update(cx, |shell, window, cx| {
            let pane = &mut shell.panes[0].as_mut().unwrap().pane;
            assert!(pane.begin("/next".into(), Default::default(), pane.now()));
            for reply in [
                flea::protocol::Reply::Listed {
                    path: "/next".into(),
                    total: 0,
                },
                flea::protocol::Reply::Rows {
                    start: 0,
                    rows: vec![],
                    listing: 3,
                },
            ] {
                shell.receive(0, BackendEvent::Reply(reply), window, cx);
            }
        })
        .unwrap();
    frame(cx, handle);
    frame(cx, handle);
}

fn zone(cx: &mut TestAppContext, handle: WindowHandle<Shell>) -> (String, String) {
    let footer = json(cx, handle, "statusFooterState");
    let text = |key| footer.get(key).unwrap().as_str().unwrap().to_owned();
    (text("path"), text("filesystem"))
}

#[gpui::test]
fn u05_disk_zone_keeps_the_last_facts_across_a_folder_change(cx: &mut TestAppContext) {
    let handle = open(cx);
    cx.simulate_window_resize(handle.into(), gpui::size(gpui::px(1100.), gpui::px(700.)));
    frame(cx, handle);
    answer(cx, handle, "/files", "btrfs");
    assert_eq!(zone(cx, handle).1, "btrfs · 442.0 GB free");
    change_folder(cx, handle);
    assert_eq!(
        zone(cx, handle),
        ("/next".into(), "btrfs · 442.0 GB free".into()),
        "the zone stays filled until the next answer replaces it"
    );
    answer(cx, handle, "/next", "ext4");
    assert_eq!(zone(cx, handle).1, "ext4 · 442.0 GB free");
}

#[gpui::test]
fn u05_a_late_answer_for_the_previous_folder_never_lands(cx: &mut TestAppContext) {
    let handle = open(cx);
    cx.simulate_window_resize(handle.into(), gpui::size(gpui::px(1100.), gpui::px(700.)));
    frame(cx, handle);
    answer(cx, handle, "/files", "btrfs");
    change_folder(cx, handle);
    answer(cx, handle, "/files", "cifs");
    assert_eq!(zone(cx, handle).1, "btrfs · 442.0 GB free");
    handle
        .update(cx, |shell, _, _| {
            let fs = &shell.surfaces[0].filesystem;
            assert_eq!(fs.path, "/next");
            assert_eq!(fs.facts.as_ref().unwrap().name, "btrfs");
            assert_eq!(fs.facts.as_ref().unwrap().path, "/files");
        })
        .unwrap();
    answer(cx, handle, "/next", "xfs");
    assert_eq!(zone(cx, handle).1, "xfs · 442.0 GB free");
}

#[gpui::test]
fn g70_fsinfo_shutdown_keeps_facts_until_the_existing_terminal_event(cx: &mut TestAppContext) {
    use crate::backend::Failure;
    let (backend, mut requests) = Backend::test_lines();
    let handle = cx.add_window(|_, cx| crate::state::tests::shell_with(backend, cx));
    frame(cx, handle);
    answer(cx, handle, "/files", "btrfs");
    while requests().is_some() {}
    handle
        .update(cx, |shell, window, cx| {
            let backend = shell.panes[0].as_ref().unwrap().backend.as_ref().unwrap();
            backend.close_stdin();
            assert_eq!(backend.fsinfo().unwrap_err().key, "window.backend_stopped");
            let pane = &mut shell.panes[0].as_mut().unwrap().pane;
            assert!(pane.begin("/next".into(), Default::default(), pane.now()));
            for reply in [
                flea::protocol::Reply::Listed {
                    path: "/next".into(),
                    total: 0,
                },
                flea::protocol::Reply::Rows {
                    start: 0,
                    rows: vec![],
                    listing: 3,
                },
            ] {
                shell.receive(0, BackendEvent::Reply(reply), window, cx);
            }
            for _ in 0..4 {
                shell.sync_footer(f32::from(window.viewport_size().width), cx);
            }
            let fs = &shell.surfaces[0].filesystem;
            assert_eq!(fs.path, "/next");
            assert!(!fs.answered);
            assert_eq!(fs.facts.as_ref().unwrap().path, "/files");
        })
        .unwrap();
    assert!(requests().is_none(), "shutdown must not enqueue fsinfo");
    assert_eq!(zone(cx, handle).1, "btrfs · 442.0 GB free");
    assert_eq!(read(cx, handle, "lastMessage"), Reply::value(""));
    handle
        .update(cx, |shell, window, cx| {
            shell.receive(
                0,
                BackendEvent::Failed(Failure {
                    key: "window.backend_stopped",
                    detail: String::new(),
                }),
                window,
                cx,
            );
            shell.receive(0, BackendEvent::Drained, window, cx);
            for _ in 0..4 {
                shell.sync_footer(f32::from(window.viewport_size().width), cx);
            }
        })
        .unwrap();
    assert_eq!(read(cx, handle, "statusError"), Reply::value("true"));
    assert_eq!(
        json(cx, handle, "statusActivityState")
            .get("errors")
            .unwrap()
            .as_f64(),
        Some(1.),
        "the existing terminal event reports failure once"
    );
    assert_eq!(zone(cx, handle).1, "btrfs · 442.0 GB free");
}
