//! SORT-0400: the mounted keyboard sort retains complete painted rows and does no echo re-list.
use super::{land, mount};
use crate::{
    backend::{Backend, Event},
    list::pane::Held,
    state::Shell,
    window::settings::store::Update,
};
use flea::{jsondoc::Json, protocol::Reply};
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::{rc::Rc, time::Duration};

fn frame(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Vec<String> {
    let previous = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().list.borrow().drawn.frames
    });
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        let list = host.list.borrow();
        assert!(
            host.drawn_list && list.drawn.frames > previous,
            "SORT-0400 frame did not draw its list"
        );
        let visible = list.visible();
        assert!(!visible.is_empty(), "SORT-0400 frame drew zero rows");
        assert!(
            visible.iter().all(|row| row.prepared.is_some()),
            "SORT-0400 frame drew partial rows: held {}+{}, pane scroll {}, drawn scroll {}, row {}, cursor {}, first missing {:?}",
            host.pane.held.start,
            host.pane.held.rows.len(),
            host.pane.scroll_y,
            list.drawn.scroll_y,
            list.drawn.metrics.row,
            host.pane.cursor,
            visible.iter().find(|row| row.prepared.is_none()).map(|row| row.index)
        );
        visible
            .iter()
            .map(|row| list.drawn_rows.row(row.index).unwrap().name.clone())
            .collect()
    })
}

fn saved_sort(shell: &Entity<Shell>, cx: &VisualTestContext) -> Json {
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        let writer = host.nav.writer.borrow();
        let patch = writer
            .patches
            .iter()
            .rev()
            .find(|patch| patch.contains("folderSorts"))
            .unwrap();
        flea::uistate::patched(
            &shell.launch.settings.data,
            &flea::jsondoc::parse(patch).unwrap(),
        )
        .unwrap()
    })
}

fn loaded(shell: &Entity<Shell>, cx: &mut VisualTestContext, document: Json) {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive_settings(
                Update::Loaded {
                    document,
                    through: 0,
                    saved: false,
                },
                window,
                cx,
            );
        })
    });
    cx.run_until_parked();
}

fn commands(requests: &mut impl FnMut() -> Option<String>) -> Vec<Json> {
    std::iter::from_fn(requests)
        .map(|line| flea::jsondoc::parse(&line).unwrap())
        .collect()
}

fn count(lines: &[Json], command: &str) -> usize {
    lines
        .iter()
        .filter(|line| line.get("c").and_then(Json::as_str) == Some(command))
        .count()
}

#[gpui::test]
fn sort0400_own_settings_echo_never_blanks_or_relists_before_or_after_sorted_rows(
    cx: &mut TestAppContext,
) {
    let (backend, mut requests) = Backend::test_lines();
    let (shell, cx) = mount(backend, cx, true);
    commands(&mut requests);
    let before = frame(&shell, cx);
    cx.simulate_keystrokes("s");
    let mut sent = commands(&mut requests);
    let document = saved_sort(&shell, cx);
    loaded(&shell, cx, document.clone());
    for _ in 0..14 {
        cx.executor().advance_clock(Duration::from_millis(10));
        cx.run_until_parked();
        assert_eq!(
            frame(&shell, cx),
            before,
            "old rows must stay through a sort longer than the listing hold cap"
        );
    }
    land(&shell, cx, "/work", 4, &["Empty", "A"]);
    assert_eq!(frame(&shell, cx), ["Empty", "A"]);
    loaded(&shell, cx, document);
    sent.extend(commands(&mut requests));
    assert_eq!(count(&sent, "sort"), 1);
    assert_eq!(
        count(&sent, "window"),
        1,
        "the sort still needs its row reply"
    );
    assert_eq!(
        count(&sent, "list"),
        0,
        "our saved folder sort must not trigger a second listing"
    );
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!((host.pane.cursor, host.pane.scroll_y), (0, 0.));
        assert!(host.selected.is_empty());
        assert!(!host.pane.busy());
    });
}

#[gpui::test]
fn sort0400_scrolled_sort_keeps_complete_old_frames_until_atomic_top_replacement(
    cx: &mut TestAppContext,
) {
    let (backend, mut requests) = Backend::test_lines();
    let (shell, cx) = mount(backend, cx, true);
    cx.update(|_, cx| {
        shell.update(cx, |shell, _| {
            let host = shell.panes[0].as_mut().unwrap();
            host.pane.held = Rc::new(Held {
                listing: 3,
                total: 2000,
                start: 0,
                rows: crate::list::bench::fixture("uniform")
                    .into_iter()
                    .take(340)
                    .collect(),
            });
        })
    });
    frame(&shell, cx);
    cx.simulate_keystrokes("shift-g");
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let host = shell.panes[0].as_mut().unwrap();
            let request = host
                .pane
                .requests
                .outstanding
                .expect("End must ask for its visible window");
            let rows = crate::list::bench::fixture("uniform")
                .into_iter()
                .skip(request.start)
                .take(request.count)
                .collect();
            host.selected = vec![1998, 1999];
            shell.receive(
                0,
                Event::Reply(Reply::Rows {
                    start: request.start,
                    listing: 3,
                    rows,
                }),
                window,
                cx,
            );
        })
    });
    let before = frame(&shell, cx);
    commands(&mut requests);
    let windows = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().pane.requests.sent
    });
    cx.simulate_keystrokes("s");
    for _ in 0..14 {
        cx.executor().advance_clock(Duration::from_millis(10));
        cx.run_until_parked();
        assert_eq!(frame(&shell, cx), before);
    }
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(
                0,
                Event::Reply(Reply::Listed {
                    path: "/work".into(),
                    total: 2000,
                }),
                window,
                cx,
            );
        })
    });
    assert_eq!(
        frame(&shell, cx),
        before,
        "the listed half must not move the viewport"
    );
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let count = shell.panes[0].as_ref().unwrap().pane.first_count();
            shell.receive(
                0,
                Event::Reply(Reply::Rows {
                    start: 0,
                    listing: 4,
                    rows: crate::list::bench::fixture("uniform")
                        .into_iter()
                        .rev()
                        .take(count)
                        .collect(),
                }),
                window,
                cx,
            );
        })
    });
    let after = frame(&shell, cx);
    assert_ne!(before, after);
    assert_eq!(after[0], "file-001999.txt");
    let sent = commands(&mut requests);
    assert_eq!(
        (
            count(&sent, "sort"),
            count(&sent, "window"),
            count(&sent, "list")
        ),
        (1, 1, 0)
    );
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(
            host.pane.requests.sent, windows,
            "render must not duplicate the sort's required top window"
        );
        assert_eq!((host.pane.cursor, host.pane.scroll_y), (0, 0.));
        assert!(host.selected.is_empty());
    });
}

#[gpui::test]
fn sort0400_external_settings_relists_without_replacing_explicit_sort(cx: &mut TestAppContext) {
    let (backend, mut requests) = Backend::test_lines();
    let (shell, cx) = mount(backend, cx, true);
    commands(&mut requests);
    cx.simulate_keystrokes("s");
    let document = saved_sort(&shell, cx);
    land(&shell, cx, "/work", 4, &["Empty", "A"]);
    loaded(&shell, cx, document.clone());
    commands(&mut requests);
    let document = flea::uistate::patched(
        &document,
        &flea::jsondoc::parse(
            r#"{"folderSorts":{"/work":{"key":"date","reverse":true}},"hidden":true}"#,
        )
        .unwrap(),
    )
    .unwrap();
    loaded(&shell, cx, document);
    let sent = commands(&mut requests);
    assert_eq!(
        count(&sent, "list"),
        1,
        "a real external update must re-list"
    );
    let list = sent
        .iter()
        .find(|line| line.get("c").and_then(Json::as_str) == Some("list"))
        .unwrap();
    assert_eq!(list.get("by").and_then(Json::as_str), Some("size"));
    assert_eq!(list.get("hidden").and_then(Json::as_bool), Some(true));
    land(&shell, cx, "/work", 5, &["A", "Empty"]);
    assert_eq!(frame(&shell, cx), ["A", "Empty"]);
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(
            host.nav.drawn_order(),
            crate::navigation::folder_state::Order {
                key: "size",
                desc: false
            }
        );
        assert_eq!(host.pane.held.listing, 5);
        assert!(!host.pane.busy());
    });
}

#[path = "explicit_sort_tests.rs"]
mod explicit;

#[gpui::test]
fn sort0400_failed_folder_save_keeps_retry_and_other_window_choices(cx: &mut TestAppContext) {
    use crate::navigation::folder_state::Writer;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    let (backend, mut requests) = Backend::test_lines();
    let (shell, cx) = mount(backend, cx, true);
    let done = Arc::new(AtomicUsize::new(0));
    cx.update(|_, cx| {
        shell.update(cx, |shell, _| {
            *shell.panes[0].as_ref().unwrap().nav.writer.borrow_mut() = Writer::held(done.clone());
        })
    });
    commands(&mut requests);
    cx.simulate_keystrokes("s");
    let saved = saved_sort(&shell, cx);
    land(&shell, cx, "/work", 4, &["Empty", "A"]);
    commands(&mut requests);
    // The folder worker has not acknowledged its failed write. A stale disk read keeps its retry.
    loaded(&shell, cx, flea::uistate::from_file("{}"));
    assert_eq!(count(&commands(&mut requests), "list"), 0);
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.nav.writer.borrow_mut().owed().len(), 1);
        assert_eq!(host.nav.drawn_order().key, "size");
    });
    assert_eq!(frame(&shell, cx), ["Empty", "A"]);
    // A successful retry retires only our patch, then another window's folder change still arrives.
    done.store(1, Ordering::Release);
    loaded(&shell, cx, saved.clone());
    assert_eq!(count(&commands(&mut requests), "list"), 0);
    let external = flea::uistate::patched(
        &saved,
        &flea::jsondoc::parse(r#"{"folderSorts":{"/other":{"key":"date","reverse":true}}}"#)
            .unwrap(),
    )
    .unwrap();
    loaded(&shell, cx, external);
    let sent = commands(&mut requests);
    assert_eq!(count(&sent, "list"), 1);
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(host.nav.writer.borrow_mut().owed().is_empty());
        assert_eq!(
            host.nav.folders.borrow().get("/other").unwrap().key,
            "mtime"
        );
    });
    land(&shell, cx, "/work", 5, &["Empty", "A"]);
    assert_eq!(frame(&shell, cx), ["Empty", "A"]);
}
