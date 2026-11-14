//! A same-folder listing keeps painted sizes while its fresh walks are outstanding.
#[path = "sizes_busy_tests.rs"]
mod busy;

use super::*;
use crate::{backend::Backend, state::tests::shell_with};
use flea::protocol::{Reply as ListingReply, RowFacts};
use gpui::{px, size, AppContext, TestAppContext};

fn rows(names: &[&str]) -> Vec<RowFacts> {
    names
        .iter()
        .map(|name| RowFacts {
            name: (*name).into(),
            dir: true,
            mode: 0o040755,
            ..Default::default()
        })
        .collect()
}

fn install(shell: &mut Shell, listing: usize, names: &[&str]) {
    let host = shell.panes[0].as_mut().unwrap();
    host.pane
        .receive(ListingReply::Listed {
            path: "/files".into(),
            total: names.len(),
        })
        .unwrap();
    host.pane
        .receive(ListingReply::Rows {
            start: 0,
            listing,
            rows: rows(names),
        })
        .unwrap();
    host.changed(&shell.locale);
    host.nav
        .after_reply(&mut host.pane, "/files", &mut host.selected);
}

#[gpui::test]
fn fxsizeblink_list_relist_keeps_sizes_until_one_changed_answer(cx: &mut TestAppContext) {
    keeps_sizes_until_one_changed_answer(cx, View::List);
}

#[gpui::test]
fn fxsizeblink_columns_relist_keeps_sizes_until_one_changed_answer(cx: &mut TestAppContext) {
    keeps_sizes_until_one_changed_answer(cx, View::Columns);
}

fn keeps_sizes_until_one_changed_answer(cx: &mut TestAppContext, view: View) {
    let (backend, _receiver) = Backend::test_channel();
    let cx = cx.add_empty_window();
    let entity = cx.update(|_, cx| cx.new(|cx| shell_with(backend, cx)));
    cx.simulate_resize(size(px(640.), px(400.)));
    cx.update(|window, cx| {
        entity.update(cx, |shell, cx| {
            {
                shell.view = view;
                shell.panes[0].as_mut().unwrap().sizes = State::default();
                install(shell, 3, &["alpha", "beta"]);
                shell.settle_list_sizes(window, cx);
                let answer = Size {
                    bytes: 120,
                    partial: false,
                };
                let host = shell.panes[0].as_mut().unwrap();
                host.sizes.waiting.push(1);
                shell.list_details(
                    0,
                    Reply::Sized {
                        listing: Some(3),
                        row: 1,
                        size: answer,
                    },
                    cx,
                );
                // A transfer removes alpha, changing beta's row number, and adds a new folder.
                install(shell, 4, &["beta", "new"]);
                shell.settle_list_sizes(window, cx);
                let host = shell.panes[0].as_mut().unwrap();
                assert_eq!(
                    host.sizes.dir_sizes().size_for(0),
                    Some(answer),
                    "{view:?}: beta must not pass through the pending mark during a re-read"
                );
                assert_eq!(host.sizes.dir_sizes().size_for(1), None);
                assert_eq!(
                    host.sizes.sizes.plan(&host.pane.held.rows, 0, 0, 1),
                    [0, 1],
                    "carrying display bytes must not suppress fresh walks"
                );
                assert_eq!(host.list.borrow().view.walked_for(4, 0), Some(answer));
                host.sizes.sizes.applied(&[0, 1]);
                host.sizes.waiting.extend([0, 1]);
                assert_eq!(host.sizes.dir_sizes().size_for(0), Some(answer));
                let revision = host.list.borrow().view.mark_revision(4);
                let changed = Size {
                    bytes: 180,
                    ..answer
                };
                for _ in 0..2 {
                    shell.list_details(
                        0,
                        Reply::Sized {
                            listing: Some(4),
                            row: 0,
                            size: changed,
                        },
                        cx,
                    );
                }
                let host = shell.panes[0].as_ref().unwrap();
                assert_eq!(host.sizes.dir_sizes().size_for(0), Some(changed));
                assert_eq!(host.list.borrow().view.mark_revision(4).1, revision.1 + 1);
            }
        })
    });
}

#[gpui::test]
fn fxsizeblink_refresh_covers_prior_watch_notice_but_keeps_later_changes(cx: &mut TestAppContext) {
    let (backend, _receiver) = Backend::test_channel();
    let cx = cx.add_empty_window();
    let entity = cx.update(|_, cx| cx.new(|cx| shell_with(backend, cx)));
    cx.update(|_, cx| {
        entity.update(cx, |shell, _| {
            install(shell, 3, &["alpha", "beta"]);
            let host = shell.panes[0].as_mut().unwrap();
            assert!(host.nav.changed(&host.pane, "/files", 0));
            assert!(host.nav.refresh(&mut host.pane, None).unwrap());
            eprintln!(
                "FX-SIZEBLINK after refresh: listRequests={} watchNotices={} stale={} due={:?}",
                host.pane.list_requests,
                host.nav.watch.notices,
                host.nav.watch.stale,
                host.nav.watch.due
            );
            assert!(
                !host.nav.watch.stale,
                "the refresh must cover the earlier notice"
            );
            assert_eq!(
                host.nav.watch.due, None,
                "no watch read stays due from the covered notice"
            );
            // A write after the request can race its read and still owes a watch refresh.
            assert!(host.nav.changed(&host.pane, "/files", 1));
            install(shell, 4, &["beta"]);
            let host = shell.panes[0].as_mut().unwrap();
            assert!(host.nav.fire_watch(&mut host.pane, &[], u64::MAX));
            assert_eq!(host.nav.watch.rereads, 1);
        });
    });
}

#[gpui::test]
fn fxsizeblink_cancelled_refresh_clears_painted_bytes_even_when_backend_stops(
    cx: &mut TestAppContext,
) {
    let (backend, _receiver) = Backend::test_channel();
    let cx = cx.add_empty_window();
    let entity = cx.update(|_, cx| cx.new(|cx| shell_with(backend, cx)));
    cx.update(|window, cx| {
        entity.update(cx, |shell, cx| {
            install(shell, 3, &["alpha"]);
            shell.settle_list_sizes(window, cx);
            let host = shell.panes[0].as_mut().unwrap();
            host.sizes.waiting.push(0);
            shell.list_details(
                0,
                Reply::Sized {
                    listing: Some(3),
                    row: 0,
                    size: Size {
                        bytes: 120,
                        partial: false,
                    },
                },
                cx,
            );
            install(shell, 4, &["alpha"]);
            shell.settle_list_sizes(window, cx);
            let host = shell.panes[0].as_mut().unwrap();
            assert!(host.list.borrow().view.walked_for(4, 0).is_some());
            host.sizes.sizes.applied(&[0]);
            host.sizes.waiting.push(0);
            host.sizes.cancel(host.backend.as_ref().unwrap());
            host.stopped = true;
            shell.settle_list_sizes(window, cx);
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.sizes.dir_sizes().size_for(0), None);
            assert_eq!(host.list.borrow().view.walked_for(4, 0), None);
        });
    });
}

#[gpui::test]
fn fxsizeblink_hidden_size_column_does_not_carry_unused_display_bytes(cx: &mut TestAppContext) {
    let (backend, _receiver) = Backend::test_channel();
    let cx = cx.add_empty_window();
    let entity = cx.update(|_, cx| cx.new(|cx| shell_with(backend, cx)));
    cx.update(|window, cx| {
        entity.update(cx, |shell, cx| {
            install(shell, 3, &["alpha"]);
            shell.settle_list_sizes(window, cx);
            let host = shell.panes[0].as_mut().unwrap();
            host.sizes.waiting.push(0);
            shell.list_details(
                0,
                Reply::Sized {
                    listing: Some(3),
                    row: 0,
                    size: Size {
                        bytes: 120,
                        partial: false,
                    },
                },
                cx,
            );
            shell.launch.settings.data =
                flea::jsondoc::parse(r#"{"columns":["modified"]}"#).unwrap();
            install(shell, 4, &["alpha"]);
            shell.settle_list_sizes(window, cx);
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.sizes.dir_sizes().size_for(0), None);
            assert_eq!(host.list.borrow().view.walked_for(4, 0), None);
        });
    });
}

#[gpui::test]
fn fxsizeblink_a_second_reread_keeps_sizes_while_the_first_walks_are_out(cx: &mut TestAppContext) {
    let (backend, _receiver) = Backend::test_channel();
    let cx = cx.add_empty_window();
    let entity = cx.update(|_, cx| cx.new(|cx| shell_with(backend, cx)));
    cx.simulate_resize(size(px(640.), px(400.)));
    cx.update(|window, cx| {
        entity.update(cx, |shell, cx| {
            let answer = Size {
                bytes: 120,
                partial: false,
            };
            install(shell, 3, &["alpha", "beta"]);
            shell.settle_list_sizes(window, cx);
            let host = shell.panes[0].as_mut().unwrap();
            host.sizes.waiting.push(1);
            shell.list_details(
                0,
                Reply::Sized {
                    listing: Some(3),
                    row: 1,
                    size: answer,
                },
                cx,
            );
            install(shell, 4, &["alpha", "beta"]);
            shell.settle_list_sizes(window, cx);
            let host = shell.panes[0].as_mut().unwrap();
            assert_eq!(host.list.borrow().view.walked_for(4, 1), Some(answer));
            host.sizes.sizes.applied(&[0, 1]);
            host.sizes.waiting.extend([0, 1]);
            // A notice that raced the first read owes a second one before beta's walk answers.
            assert!(host.nav.refresh(&mut host.pane, None).unwrap());
            assert!(host.pane.busy());
            shell.settle_list_sizes(window, cx);
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(
                host.list.borrow().view.walked_for(4, 1),
                Some(answer),
                "beta must not pass through the pending mark while the second read is out"
            );
            install(shell, 5, &["alpha", "beta"]);
            shell.settle_list_sizes(window, cx);
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.sizes.dir_sizes().size_for(1), Some(answer));
            assert_eq!(host.list.borrow().view.walked_for(5, 1), Some(answer));
        });
    });
}
