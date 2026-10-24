//! Closing proof for successive navigations while a Grid exit is staged.
use super::join_tests::{frame, key, named_shell};
use crate::{
    backend::{Backend, Event},
    state::Shell,
    window::startup::View,
};
use flea::protocol;
use gpui::{TestAppContext, WindowHandle};

fn land(
    cx: &mut TestAppContext,
    handle: WindowHandle<Shell>,
    path: &str,
    listing: usize,
    start: usize,
) {
    handle
        .update(cx, |shell, window, cx| {
            let first = {
                let pane = &mut shell.panes[0].as_mut().unwrap().pane;
                assert!(pane.begin(
                    path.into(),
                    crate::pane::swap::Options::default(),
                    pane.now()
                ));
                pane.swap.pending.as_ref().unwrap().first
            };
            shell.receive(
                0,
                Event::Reply(protocol::Reply::Listed {
                    path: path.into(),
                    total: 1000,
                }),
                window,
                cx,
            );
            shell.receive(
                0,
                Event::Reply(protocol::Reply::Rows {
                    start,
                    listing,
                    rows: (start..(start + first).min(1000))
                        .map(|index| protocol::RowFacts {
                            name: format!("file-{index}.txt"),
                            ..Default::default()
                        })
                        .collect(),
                }),
                window,
                cx,
            );
            assert!(
                !shell.panes[0].as_ref().unwrap().pane.busy(),
                "complete pair committed"
            );
        })
        .unwrap();
}

#[gpui::test]
fn rev_u10_r3_a_second_navigation_retires_the_first_superseded_windows_refusal(
    cx: &mut TestAppContext,
) {
    let (backend, _requests) = Backend::test_channel();
    let handle = cx.add_window(|_, cx| named_shell(backend, cx));
    handle
        .update(cx, |shell, window, cx| {
            shell.set_view(View::Grid, window, cx)
        })
        .unwrap();
    frame(cx, handle);
    key(cx, handle, "end");
    handle
        .update(cx, |shell, window, cx| {
            shell.set_view(View::List, window, cx)
        })
        .unwrap();
    frame(cx, handle);
    // A selected last entry is a legal first window; the taller List needs its preceding rows.
    land(cx, handle, "/first", 3, 999);
    frame(cx, handle);
    handle
        .update(cx, |shell, window, cx| {
            assert_eq!(shell.view, View::Grid);
            let pane = &shell.panes[0].as_ref().unwrap().pane;
            assert_eq!(pane.held.listing, 3);
            assert_eq!(pane.cursor, 999);
            assert!(
                pane.requests.outstanding.is_some(),
                "destination window requested"
            );
            shell.receive(
                0,
                Event::Refused {
                    operation: "window".into(),
                    path: "/first".into(),
                    message: "Permission denied".into(),
                    mode: 0,
                },
                window,
                cx,
            );
        })
        .unwrap();
    // The new navigation completes before the redraw queued by that old refusal.
    land(cx, handle, "/second", 4, 0);
    frame(cx, handle);
    handle.update(cx, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(shell.view, View::List, "operator's requested view still lands");
        assert_eq!(host.pane.state.kind, crate::pane::Kind::Ready,
            "a successful second navigation inherited the first listing's refusal: state {:?}, error {:?}, held listing {}",
            host.pane.state.kind, host.error, host.pane.held.listing);
        assert_eq!(host.pane.path(), "/second");
        assert!(host.list.borrow().visible().iter().all(|row| row.prepared.is_some()));
    }).unwrap();
}

#[gpui::test]
fn rev_u10_r3_a_coalesced_second_navigation_retires_the_previous_refusal(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let handle = cx.add_window(|_, cx| named_shell(backend, cx));
    handle
        .update(cx, |shell, window, cx| {
            shell.set_view(View::Grid, window, cx)
        })
        .unwrap();
    frame(cx, handle);
    key(cx, handle, "end");
    handle
        .update(cx, |shell, window, cx| {
            shell.set_view(View::List, window, cx)
        })
        .unwrap();
    frame(cx, handle);
    land(cx, handle, "/first", 3, 999);
    frame(cx, handle);
    handle
        .update(cx, |shell, window, cx| {
            assert_eq!(shell.view, View::Grid);
            assert!(shell.panes[0]
                .as_ref()
                .unwrap()
                .pane
                .requests
                .outstanding
                .is_some());
            shell.receive(
                0,
                Event::Refused {
                    operation: "window".into(),
                    path: "/first".into(),
                    message: "Permission denied".into(),
                    mode: 0,
                },
                window,
                cx,
            );
            let first = {
                let pane = &mut shell.panes[0].as_mut().unwrap().pane;
                assert!(pane.begin(
                    "/second".into(),
                    crate::pane::swap::Options::default(),
                    pane.now()
                ));
                pane.swap.pending.as_ref().unwrap().first
            };
            shell.receive(
                0,
                Event::Reply(protocol::Reply::Listed {
                    path: "/second".into(),
                    total: 1000,
                }),
                window,
                cx,
            );
            shell.receive(
                0,
                Event::Reply(protocol::Reply::Rows {
                    start: 0,
                    listing: 4,
                    rows: (0..first)
                        .map(|index| protocol::RowFacts {
                            name: format!("file-{index}.txt"),
                            ..Default::default()
                        })
                        .collect(),
                }),
                window,
                cx,
            );
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.pane.state.kind, crate::pane::Kind::Ready);
            assert_eq!(host.pane.path(), "/second");
            assert_eq!(host.pane.held.listing, 4);
        })
        .unwrap();
    frame(cx, handle);
    handle.update(cx, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(shell.view, View::List);
        assert_eq!(host.pane.state.kind, crate::pane::Kind::Ready,
            "a complete second navigation inherited a stale refusal: state {:?}, error {:?}, held listing {}",
            host.pane.state.kind, host.error, host.pane.held.listing);
        assert_eq!(host.pane.path(), "/second");
        assert!(host.list.borrow().visible().iter().all(|row| row.prepared.is_some()));
    }).unwrap();
}

#[gpui::test]
fn rev_u10_r3_live_reload_and_navigation_retire_the_previous_refusal(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let handle = cx.add_window(|_, cx| named_shell(backend, cx));
    handle
        .update(cx, |shell, window, cx| {
            // The fixture installed its initial pair directly; sync the navigator as Shell::receive does.
            let host = shell.panes[0].as_mut().unwrap();
            host.nav
                .after_reply(&mut host.pane, "/files", &mut host.selected);
            shell.set_view(View::Grid, window, cx);
        })
        .unwrap();
    frame(cx, handle);
    key(cx, handle, "end");
    handle
        .update(cx, |shell, window, cx| {
            shell.set_view(View::List, window, cx)
        })
        .unwrap();
    frame(cx, handle);
    handle
        .update(cx, |shell, window, cx| {
            assert!(shell
                .step(0, crate::window::navigate::Step::Reload, window, cx)
                .unwrap());
            let first = shell.panes[0]
                .as_ref()
                .unwrap()
                .pane
                .swap
                .pending
                .as_ref()
                .unwrap()
                .first;
            shell.receive(
                0,
                Event::Reply(protocol::Reply::Listed {
                    path: "/files".into(),
                    total: 1000,
                }),
                window,
                cx,
            );
            shell.receive(
                0,
                Event::Reply(protocol::Reply::Rows {
                    start: 0,
                    listing: 3,
                    rows: (0..first)
                        .map(|index| protocol::RowFacts {
                            name: format!("file-{index}.txt"),
                            ..Default::default()
                        })
                        .collect(),
                }),
                window,
                cx,
            );
        })
        .unwrap();
    frame(cx, handle);
    handle
        .update(cx, |shell, window, cx| {
            assert_eq!(shell.view, View::Grid);
            let pane = &shell.panes[0].as_ref().unwrap().pane;
            assert_eq!(pane.held.listing, 3);
            assert_eq!(pane.held.start, 0);
            assert_eq!(
                pane.cursor, 999,
                "the live refresh anchor restores the old index"
            );
            assert!(pane.requests.outstanding.is_some());
            shell.receive(
                0,
                Event::Refused {
                    operation: "window".into(),
                    path: "/files".into(),
                    message: "Permission denied".into(),
                    mode: 0,
                },
                window,
                cx,
            );
            assert!(shell.navigate(
                0,
                "/second".into(),
                None,
                crate::pane::swap::Options::default(),
                window,
                cx
            ));
            let first = shell.panes[0]
                .as_ref()
                .unwrap()
                .pane
                .swap
                .pending
                .as_ref()
                .unwrap()
                .first;
            shell.receive(
                0,
                Event::Reply(protocol::Reply::Listed {
                    path: "/second".into(),
                    total: 1000,
                }),
                window,
                cx,
            );
            shell.receive(
                0,
                Event::Reply(protocol::Reply::Rows {
                    start: 0,
                    listing: 4,
                    rows: (0..first)
                        .map(|index| protocol::RowFacts {
                            name: format!("file-{index}.txt"),
                            ..Default::default()
                        })
                        .collect(),
                }),
                window,
                cx,
            );
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.pane.state.kind, crate::pane::Kind::Ready);
            assert_eq!(host.pane.path(), "/second");
            assert_eq!(host.pane.held.listing, 4);
        })
        .unwrap();
    frame(cx, handle);
    handle.update(cx, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(shell.view, View::List);
        assert_eq!(host.pane.state.kind, crate::pane::Kind::Ready,
            "a complete live navigation inherited the refresh window's refusal: state {:?}, error {:?}, held listing {}",
            host.pane.state.kind, host.error, host.pane.held.listing);
        assert_eq!(host.pane.path(), "/second");
        assert!(host.list.borrow().visible().iter().all(|row| row.prepared.is_some()));
    }).unwrap();
}
