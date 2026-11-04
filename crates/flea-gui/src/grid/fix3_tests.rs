//! Live controls for refusal ownership across successive staged destinations.
use super::join_tests::{frame, key, named_shell};
use crate::{
    backend::{Backend, Event},
    state::Shell,
    window::{navigate::Step, startup::View},
};
use flea::protocol;
use gpui::{Context, TestAppContext, Window, WindowHandle};

fn reload(shell: &mut Shell, window: &mut Window, cx: &mut Context<Shell>, listing: usize) {
    assert!(shell.step(0, Step::Reload, window, cx).unwrap());
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
            listing,
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
}

fn stage(cx: &mut TestAppContext, handle: WindowHandle<Shell>) {
    handle
        .update(cx, |shell, window, cx| {
            let host = shell.panes[0].as_mut().unwrap();
            assert_eq!(host.pane.held.rows.len(), 320);
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
        .update(cx, |shell, window, cx| reload(shell, window, cx, 3))
        .unwrap();
    frame(cx, handle);
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(shell.view, View::Grid);
            assert_eq!(host.pane.held.listing, 3);
            assert_eq!(host.pane.held.start, 0);
            assert_eq!(host.pane.cursor, 999);
            assert_eq!(host.pane.requests.outstanding.unwrap().listing, 3);
            assert!(host.grid.as_ref().unwrap().next_view.is_some());
        })
        .unwrap();
}

fn refuse(shell: &mut Shell, window: &mut Window, cx: &mut Context<Shell>, message: &str) {
    let host = shell.panes[0].as_ref().unwrap();
    assert_eq!(
        host.pane.requests.outstanding.unwrap().listing,
        host.pane.held.listing
    );
    shell.receive(
        0,
        Event::Refused {
            operation: "window".into(),
            path: "/files".into(),
            message: message.into(),
            mode: 0,
        },
        window,
        cx,
    );
}

#[gpui::test]
fn u10_fix3_live_sequential_refusal_then_navigation_still_lands_ready(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let handle = cx.add_window(|_, cx| named_shell(backend, cx));
    stage(cx, handle);
    handle
        .update(cx, |shell, window, cx| {
            refuse(shell, window, cx, "Permission denied")
        })
        .unwrap();
    frame(cx, handle);
    handle
        .update(cx, |shell, window, cx| {
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(shell.view, View::List);
            assert_eq!(host.pane.state.kind, crate::pane::Kind::Error);
            assert_eq!(host.error.as_deref(), Some("Permission denied"));
            assert!(shell.navigate(
                0,
                "/second".into(),
                None,
                crate::pane::swap::Options::default(),
                window,
                cx,
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
        })
        .unwrap();
    frame(cx, handle);
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(shell.view, View::List);
            assert_eq!(host.pane.state.kind, crate::pane::Kind::Ready);
            assert_eq!(host.pane.path(), "/second");
            assert_eq!(host.pane.held.listing, 4);
            assert!(!host.list.borrow().visible().is_empty());
            assert!(host
                .list
                .borrow()
                .visible()
                .iter()
                .all(|row| row.prepared.is_some()));
        })
        .unwrap();
}

#[gpui::test]
fn u10_fix3_a_genuine_second_generation_refusal_keeps_its_own_reason(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let handle = cx.add_window(|_, cx| named_shell(backend, cx));
    stage(cx, handle);
    handle
        .update(cx, |shell, window, cx| {
            refuse(shell, window, cx, "Previous listing refused");
            reload(shell, window, cx, 4);
        })
        .unwrap();
    frame(cx, handle);
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(shell.view, View::Grid);
            assert_eq!(host.pane.state.kind, crate::pane::Kind::Ready);
            assert_eq!(host.pane.held.listing, 4);
            assert_eq!(host.pane.cursor, 999);
            assert_eq!(host.pane.requests.outstanding.unwrap().listing, 4);
            assert!(host.grid.as_ref().unwrap().next_view.is_some());
            assert_eq!(
                host.grid
                    .as_ref()
                    .unwrap()
                    .state
                    .borrow()
                    .frame_held
                    .as_ref()
                    .unwrap()
                    .listing,
                2,
                "the staged Grid still draws its original rows"
            );
        })
        .unwrap();
    handle
        .update(cx, |shell, window, cx| {
            refuse(shell, window, cx, "Current listing refused")
        })
        .unwrap();
    frame(cx, handle);
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(shell.view, View::List);
            assert_eq!(host.pane.state.kind, crate::pane::Kind::Error);
            assert_eq!(host.pane.path(), "/files");
            assert_eq!(host.error.as_deref(), Some("Current listing refused"));
            assert!(host.pane.held.rows.is_empty());
            assert!(host.grid.as_ref().unwrap().next_view.is_none());
            assert!(shell.slots.states[0].is_some());
            assert!(host.list.borrow().visible().is_empty());
        })
        .unwrap();
}
