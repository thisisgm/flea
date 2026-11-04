//! IPC while the startup listing is held: refused at once with Quickshell's answer, served once the listing answers.
use crate::{
    backend::{Backend, Event as BackendEvent},
    ipc::server::{Answer, Event},
    state::Shell,
    window::{
        locale::Snapshot,
        startup::{Arguments, Launch, Settings},
    },
};
use flea::{ipc::wire::Request, protocol::Reply};
use gpui::{TestAppContext, WindowHandle};
use std::{
    sync::{mpsc, Arc},
    time::Duration,
};

// The real startup constructor over a test backend whose list request is never answered.
fn open(cx: &mut TestAppContext) -> (WindowHandle<Shell>, async_channel::Sender<Event>) {
    cx.update(|cx| {
        cx.set_global(crate::state::constructor_fixture::Services {
            backend: |_, cx| {
                let (backend, requests) = Backend::test_channel();
                cx.on_release(move |_, _| drop(requests)).detach();
                Ok(backend)
            },
            monitor: None,
        })
    });
    let (send, events) = async_channel::bounded(4);
    let handle = cx.add_window(|window, cx| {
        let locale = Snapshot {
            revision: 1,
            catalog: Arc::new(crate::test_catalogs::english()),
        };
        let settings = Settings::from_json(flea::jsondoc::parse("{}").unwrap());
        let launch = Launch::resolve(
            settings,
            Arguments {
                path: Some("/files".into()),
                select: None,
            },
            "/home/test",
        );
        Shell::new(launch, locale, events, window, cx)
    });
    (handle, send)
}

fn call(send: &async_channel::Sender<Event>, function: &str) -> mpsc::Receiver<Answer> {
    let (answer, receive) = mpsc::sync_channel(1);
    send.try_send(Event::Call {
        request: Request::Call {
            target: "flea".into(),
            function: function.into(),
            arguments: vec![],
        },
        answer,
    })
    .unwrap();
    receive
}

#[gpui::test]
fn a_call_during_a_held_first_listing_is_refused_at_once_and_never_run_late(
    cx: &mut TestAppContext,
) {
    let (handle, send) = open(cx);
    let early = call(&send, "state");
    cx.run_until_parked();
    let refused = early
        .try_recv()
        .expect("a call while the first listing is held gets a reply line at once");
    assert_eq!(refused.reply, flea::ipc::registry::no_target());
    // Past the server's two second client wait the listing is still held and the window still answers at once.
    cx.executor().advance_clock(Duration::from_millis(2500));
    let held = call(&send, "bodyPx");
    cx.run_until_parked();
    assert_eq!(
        held.try_recv().unwrap().reply,
        flea::ipc::registry::no_target()
    );
    let kind = handle
        .update(cx, |shell, _, _| {
            shell.panes[0].as_ref().unwrap().pane.state.kind
        })
        .unwrap();
    assert_eq!(
        kind,
        crate::pane::states::Kind::Loading,
        "only the ten second deadline turns a held listing to waiting"
    );
    handle
        .update(cx, |shell, window, cx| {
            shell.receive(
                0,
                BackendEvent::Reply(Reply::Listed {
                    path: "/files".into(),
                    total: 7,
                }),
                window,
                cx,
            );
            shell.receive(
                0,
                BackendEvent::Reply(Reply::Rows {
                    start: 0,
                    rows: vec![flea::protocol::RowFacts::default(); 7],
                    listing: 1,
                }),
                window,
                cx,
            );
        })
        .unwrap();
    cx.run_until_parked();
    let served = call(&send, "path");
    cx.run_until_parked();
    assert_eq!(
        served.try_recv().unwrap().reply,
        flea::ipc::wire::Reply::value("/files")
    );
}

#[gpui::test]
fn the_waiting_deadline_opens_ipc_over_a_listing_that_never_answers(cx: &mut TestAppContext) {
    let (_handle, send) = open(cx);
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(
        crate::window::startup::LISTING_WAIT_MS,
    ));
    cx.run_until_parked();
    let state = call(&send, "state");
    cx.run_until_parked();
    assert_eq!(
        state.try_recv().unwrap().reply,
        flea::ipc::wire::Reply::value("waiting")
    );
}

fn first_listing(cx: &mut TestAppContext, handle: WindowHandle<Shell>, side: usize) {
    handle
        .update(cx, |shell, window, cx| {
            let path = shell.panes[side].as_ref().unwrap().pane.place().to_owned();
            shell.receive(
                side,
                BackendEvent::Reply(Reply::Listed { path, total: 1 }),
                window,
                cx,
            );
            shell.receive(
                side,
                BackendEvent::Reply(Reply::Rows {
                    start: 0,
                    rows: vec![flea::protocol::RowFacts::default()],
                    listing: 1,
                }),
                window,
                cx,
            );
        })
        .unwrap();
}

#[gpui::test]
fn a_deferred_first_listing_opens_ipc_only_after_side_zero_settles(cx: &mut TestAppContext) {
    let (handle, send) = open(cx);
    cx.run_until_parked();
    handle
        .update(cx, |shell, window, cx| {
            shell.pane_mark_shown(0, true, window, cx);
        })
        .unwrap();
    first_listing(cx, handle, 0);
    cx.executor()
        .advance_clock(Duration::from_millis(crate::pane::swap::CAP_MS));
    handle
        .update(cx, |shell, window, cx| {
            shell.advance_transition(0, window, cx);
            let host = shell.panes[0].as_ref().unwrap();
            assert!(
                host.pane.swap.fell_back,
                "the tick changed an unsettled pane"
            );
            assert!(host.pane.swap.pending.as_ref().unwrap().ready.is_some());
            assert!(host.pane.busy());
            assert!(!host.answered);
            assert_eq!(host.pane.held.listing, 0);
            assert!(shell.deadlines[0].is_some());
            assert!(shell.ipc_cold);
        })
        .unwrap();
    let pending = call(&send, "bodyPx");
    cx.run_until_parked();
    assert_eq!(
        pending.try_recv().unwrap().reply,
        flea::ipc::registry::no_target()
    );
    cx.executor().advance_clock(Duration::from_millis(
        crate::pane::swap::MIN_SHOW_MS - crate::pane::swap::CAP_MS,
    ));
    cx.run_until_parked();
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert!(!host.pane.busy());
            assert!(host.answered);
            assert_eq!(host.pane.held.listing, 1);
            assert_eq!(host.pane.held.total, 1);
            assert!(shell.deadlines[0].is_none());
        })
        .unwrap();
    let path = call(&send, "path");
    let body = call(&send, "bodyPx");
    cx.run_until_parked();
    assert_eq!(
        path.try_recv().unwrap().reply,
        flea::ipc::wire::Reply::value("/files")
    );
    assert_eq!(
        body.try_recv().unwrap().reply,
        flea::ipc::wire::Reply::value("14")
    );
}

#[gpui::test]
fn a_deferred_side_one_listing_does_not_open_ipc_for_a_held_side_zero(cx: &mut TestAppContext) {
    let (handle, send) = open(cx);
    cx.run_until_parked();
    handle
        .update(cx, |shell, window, cx| {
            shell.set_view(super::startup::View::Dual, window, cx);
            shell.pane_mark_shown(1, true, window, cx);
        })
        .unwrap();
    first_listing(cx, handle, 1);
    handle
        .update(cx, |shell, _, _| {
            assert!(shell.panes[1].as_ref().unwrap().pane.busy());
        })
        .unwrap();
    cx.executor()
        .advance_clock(Duration::from_millis(crate::pane::swap::MIN_SHOW_MS));
    cx.run_until_parked();
    handle
        .update(cx, |shell, _, _| {
            let side_one = shell.panes[1].as_ref().unwrap();
            assert!(!side_one.pane.busy());
            assert!(side_one.answered);
            assert_eq!(side_one.pane.held.listing, 1);
            let side_zero = shell.panes[0].as_ref().unwrap();
            assert!(side_zero.pane.busy());
            assert!(!side_zero.answered);
            assert!(shell.deadlines[0].is_some());
            assert!(shell.ipc_cold);
        })
        .unwrap();
    let cold = call(&send, "bodyPx");
    cx.run_until_parked();
    assert_eq!(
        cold.try_recv().unwrap().reply,
        flea::ipc::registry::no_target()
    );
}

#[gpui::test]
fn vf_readers_delayed_first_listing_opens_ipc_for_painted_rows(cx: &mut TestAppContext) {
    let (handle, send) = open(cx);
    handle
        .update(cx, |shell, window, cx| {
            let pane = &mut shell.panes[0].as_mut().unwrap().pane;
            pane.swap.mark_shown(true, pane.now());
            for reply in [
                Reply::Listed {
                    path: "/files".into(),
                    total: 1,
                },
                Reply::Rows {
                    start: 0,
                    rows: vec![flea::protocol::RowFacts::default()],
                    listing: 1,
                },
            ] {
                shell.receive(0, BackendEvent::Reply(reply), window, cx);
            }
            assert!(
                shell.panes[0].as_ref().unwrap().pane.busy(),
                "first screen holds the drawn loading minimum"
            );
            assert!(shell.ipc_cold);
        })
        .unwrap();
    cx.executor().advance_clock(Duration::from_millis(200));
    handle
        .update(cx, |shell, window, cx| {
            shell.advance_transition(0, window, cx);
            assert_eq!(shell.panes[0].as_ref().unwrap().pane.held.total, 1);
            assert!(
                !shell.ipc_cold,
                "a delayed commit must open the IPC reader with the painted rows"
            );
        })
        .unwrap();
    let served = call(&send, "bodyPx");
    cx.run_until_parked();
    assert_eq!(served.try_recv().unwrap().reply.code, 0);
}
