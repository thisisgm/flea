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
            catalog: Arc::new(flea::i18n::I18n::default()),
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
