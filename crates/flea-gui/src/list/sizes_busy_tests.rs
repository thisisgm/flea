//! Busy same-folder reads retain display facts, while terminal paths leave no stale cells.
use super::*;

const ANSWER: Size = Size {
    bytes: 120,
    partial: false,
};

#[gpui::test]
fn fxsizeblink_a_queued_reread_adopts_sizes_before_its_first_busy_frame(cx: &mut TestAppContext) {
    let (backend, _receiver) = Backend::test_channel();
    let cx = cx.add_empty_window();
    let entity = cx.update(|_, cx| cx.new(|cx| shell_with(backend, cx)));
    cx.update(|window, cx| {
        entity.update(cx, |shell, cx| {
            carried(shell, window, cx);
            let host = shell.panes[0].as_mut().unwrap();
            assert!(host.nav.refresh(&mut host.pane, None).unwrap());
            shell.settle_list_sizes(window, cx);
            install(shell, 5, &["beta", "new"]);
            // A queued refresh starts from the reply handler before the next frame can adopt listing 5.
            let host = shell.panes[0].as_mut().unwrap();
            assert!(host.nav.refresh(&mut host.pane, None).unwrap());
            assert!(host.pane.busy());
            shell.settle_list_sizes(window, cx);
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.list.borrow().view.walked_for(5, 0), Some(ANSWER));
            assert_eq!(host.sizes.dir_sizes().size_for(1), None);
        });
    });
}

fn carried(shell: &mut Shell, window: &Window, cx: &mut Context<Shell>) {
    install(shell, 3, &["alpha", "beta"]);
    shell.settle_list_sizes(window, cx);
    for row in [0, 1] {
        shell.panes[0].as_mut().unwrap().sizes.waiting.push(row);
        shell.list_details(
            0,
            Reply::Sized {
                listing: Some(3),
                row,
                size: ANSWER,
            },
            cx,
        );
    }
    install(shell, 4, &["alpha", "beta"]);
    shell.settle_list_sizes(window, cx);
    let host = shell.panes[0].as_mut().unwrap();
    host.sizes.sizes.applied(&[0, 1]);
    host.sizes.waiting.extend([0, 1]);
    assert_eq!(host.list.borrow().view.walked_for(4, 1), Some(ANSWER));
}

#[gpui::test]
fn fxsizeblink_two_busy_rereads_change_once_when_the_fresh_walk_answers(cx: &mut TestAppContext) {
    let (backend, _receiver) = Backend::test_channel();
    let cx = cx.add_empty_window();
    let entity = cx.update(|_, cx| cx.new(|cx| shell_with(backend, cx)));
    cx.update(|window, cx| {
        entity.update(cx, |shell, cx| {
            carried(shell, window, cx);
            // Alpha has answered on the current listing; beta still shows its provisional bytes.
            shell.list_details(
                0,
                Reply::Sized {
                    listing: Some(4),
                    row: 0,
                    size: ANSWER,
                },
                cx,
            );
            for listing in [5, 6] {
                let host = shell.panes[0].as_mut().unwrap();
                assert!(host.nav.refresh(&mut host.pane, None).unwrap());
                shell.settle_list_sizes(window, cx);
                let host = shell.panes[0].as_ref().unwrap();
                for row in [0, 1] {
                    assert_eq!(host.sizes.dir_sizes().size_for(row), Some(ANSWER));
                    assert_eq!(
                        host.list.borrow().view.walked_for(listing - 1, row),
                        Some(ANSWER)
                    );
                }
                install(shell, listing, &["alpha", "beta"]);
                shell.settle_list_sizes(window, cx);
                let host = shell.panes[0].as_mut().unwrap();
                for row in [0, 1] {
                    assert_eq!(host.sizes.dir_sizes().size_for(row), Some(ANSWER));
                    assert_eq!(
                        host.list.borrow().view.walked_for(listing, row),
                        Some(ANSWER)
                    );
                }
                assert_eq!(host.sizes.sizes.plan(&host.pane.held.rows, 0, 0, 1), [0, 1]);
                host.sizes.sizes.applied(&[0, 1]);
                host.sizes.waiting.extend([0, 1]);
            }
            let revision = shell.panes[0]
                .as_ref()
                .unwrap()
                .list
                .borrow()
                .view
                .mark_revision(6);
            let changed = Size {
                bytes: 180,
                ..ANSWER
            };
            for _ in 0..2 {
                shell.list_details(
                    0,
                    Reply::Sized {
                        listing: Some(6),
                        row: 1,
                        size: changed,
                    },
                    cx,
                );
            }
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.sizes.dir_sizes().size_for(1), Some(changed));
            assert_eq!(host.list.borrow().view.mark_revision(6).1, revision.1 + 1);
        });
    });
}

#[gpui::test]
fn fxsizeblink_real_cancels_still_drop_provisional_bytes_during_a_reread(cx: &mut TestAppContext) {
    for reason in [
        "stopped",
        "drained",
        "search",
        "switched",
        "hidden",
        "navigation",
    ] {
        let (backend, _receiver) = Backend::test_channel();
        let cx = cx.add_empty_window();
        let entity = cx.update(|_, cx| cx.new(|cx| shell_with(backend, cx)));
        cx.update(|window, cx| {
            entity.update(cx, |shell, cx| {
                carried(shell, window, cx);
                let host = shell.panes[0].as_mut().unwrap();
                assert!(host.nav.refresh(&mut host.pane, None).unwrap());
                shell.settle_list_sizes(window, cx);
                let host = shell.panes[0].as_mut().unwrap();
                assert_eq!(host.sizes.dir_sizes().size_for(1), Some(ANSWER), "{reason}");
                match reason {
                    "stopped" => host.stopped = true,
                    "drained" => {
                        host.stopped = true;
                        host.backend = None;
                    }
                    "search" => host.pane.state.search = true,
                    "switched" => shell.view = View::Grid,
                    "hidden" => shell.panes[1] = shell.panes[0].take(),
                    "navigation" => {
                        assert!(host.nav.open(&mut host.pane, "/other").unwrap());
                    }
                    _ => unreachable!(),
                }
                let side = usize::from(reason == "hidden");
                shell.sync_list_sizes(side, window, cx);
                let host = shell.panes[side].as_ref().unwrap();
                assert_eq!(host.sizes.dir_sizes().size_for(1), None, "{reason}");
                assert_eq!(host.list.borrow().view.walked_for(4, 1), None, "{reason}");
            });
        });
    }
}

#[gpui::test]
fn fxsizeblink_failed_read_clears_rows_and_retry_reads_the_covered_change(cx: &mut TestAppContext) {
    let (backend, _receiver) = Backend::test_channel();
    let cx = cx.add_empty_window();
    let entity = cx.update(|_, cx| cx.new(|cx| shell_with(backend, cx)));
    cx.update(|window, cx| {
        entity.update(cx, |shell, cx| {
            carried(shell, window, cx);
            let host = shell.panes[0].as_mut().unwrap();
            let before = host.pane.list_requests;
            assert!(host.nav.changed(&host.pane, "/files", 0));
            assert!(host.nav.refresh(&mut host.pane, None).unwrap());
            assert!(!host.nav.watch.stale);
            assert_eq!(host.nav.watch.due, None);
            shell.settle_list_sizes(window, cx);
            shell.receive(
                0,
                crate::backend::Event::Refused {
                    operation: "scan".into(),
                    path: "/files".into(),
                    message: "Permission denied".into(),
                    mode: 0,
                },
                window,
                cx,
            );
            shell.settle_list_sizes(window, cx);
            let host = shell.panes[0].as_mut().unwrap();
            assert_eq!(host.pane.state.kind, crate::pane::Kind::Locked);
            assert!(host.pane.held.rows.is_empty());
            assert_eq!(host.sizes.dir_sizes().size_for(1), None);
            assert_eq!(host.list.borrow().view.walked_for(4, 1), None);
            assert!(!host.nav.fire_watch(&mut host.pane, &[], u64::MAX));
            assert!(host.nav.refresh(&mut host.pane, None).unwrap());
            assert_eq!(host.pane.list_requests, before + 2);
            install(shell, 5, &["beta", "new"]);
            shell.settle_list_sizes(window, cx);
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.pane.held.rows[0].name, "beta");
            assert_eq!(host.sizes.dir_sizes().size_for(0), None);
            assert_eq!(host.sizes.sizes.plan(&host.pane.held.rows, 0, 0, 1), [0, 1]);
        });
    });
}

#[gpui::test]
fn fxsizeblink_superseded_read_fetches_the_change_on_return(cx: &mut TestAppContext) {
    let (backend, _receiver) = Backend::test_channel();
    let cx = cx.add_empty_window();
    let entity = cx.update(|_, cx| cx.new(|cx| shell_with(backend, cx)));
    cx.update(|window, cx| {
        entity.update(cx, |shell, cx| {
            carried(shell, window, cx);
            let host = shell.panes[0].as_mut().unwrap();
            let before = host.pane.list_requests;
            assert!(host.nav.changed(&host.pane, "/files", 0));
            assert!(host.nav.refresh(&mut host.pane, None).unwrap());
            assert!(!host.nav.watch.stale);
            assert_eq!(host.nav.watch.due, None);
            shell.settle_list_sizes(window, cx);
            let host = shell.panes[0].as_mut().unwrap();
            assert!(host.nav.open(&mut host.pane, "/other").unwrap());
            shell.settle_list_sizes(window, cx);
            let host = shell.panes[0].as_mut().unwrap();
            assert_eq!(host.sizes.dir_sizes().size_for(1), None);
            assert_eq!(host.list.borrow().view.walked_for(4, 1), None);
            host.pane
                .receive(ListingReply::Listed {
                    path: "/other".into(),
                    total: 1,
                })
                .unwrap();
            host.pane
                .receive(ListingReply::Rows {
                    start: 0,
                    listing: 5,
                    rows: rows(&["away"]),
                })
                .unwrap();
            host.changed(&shell.locale);
            host.nav
                .after_reply(&mut host.pane, "/files", &mut host.selected);
            assert_eq!(host.pane.path(), "/other");
            assert!(host.nav.open(&mut host.pane, "/files").unwrap());
            assert_eq!(host.pane.list_requests, before + 3);
            install(shell, 6, &["beta", "new"]);
            shell.settle_list_sizes(window, cx);
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.pane.held.rows[0].name, "beta");
            assert_eq!(host.sizes.dir_sizes().size_for(0), None);
            assert_eq!(host.sizes.sizes.plan(&host.pane.held.rows, 0, 0, 1), [0, 1]);
        });
    });
}
