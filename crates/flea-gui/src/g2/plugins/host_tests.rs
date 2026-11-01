use super::*;
use gpui::TestAppContext;
use std::{cell::RefCell, rc::Rc};

const PLUGIN: &str = "example.checksums";

struct Fixture {
    host: Entity<Host>,
    output: async_channel::Sender<Event>,
    lines: Box<dyn FnMut() -> Option<String>>,
}

fn fixture(cx: &mut TestAppContext) -> Fixture {
    let (backend, mut lines) = Backend::test_lines();
    let output = backend.test_output();
    let host = cx.new(Host::new);
    host.update(cx, |host, cx| host.test_attach(backend, cx));
    assert_eq!(lines(), Some(wire::request_line(0, "list", "", Vec::new())));
    Fixture {
        host,
        output,
        lines: Box::new(lines),
    }
}

fn request(f: &mut Fixture, cx: &mut TestAppContext) -> (u64, async_channel::Receiver<Response>) {
    let receiver = f.host.update(cx, |host, cx| {
        host.ready.insert(PLUGIN.into());
        host.answers
            .insert((PLUGIN.into(), "targets".into()), Json::Null);
        host.request_wait(
            PLUGIN,
            Json::Obj(vec![("op".into(), Json::Str("run".into()))]),
            cx,
        )
        .unwrap()
    });
    let id = f.host.read_with(cx, |host, _| host.next);
    assert!((f.lines)().is_some(), "the request reached the owner pipe");
    (id, receiver)
}

fn reply(f: &Fixture, line: String, cx: &mut TestAppContext) {
    f.output.try_send(Event::Plugins(line)).unwrap();
    cx.run_until_parked();
}

#[gpui::test]
fn plugin_host_progress_preserves_waiter_until_terminal_reply(cx: &mut TestAppContext) {
    let mut f = fixture(cx);
    let responses = Rc::new(RefCell::new(Vec::new()));
    let seen = responses.clone();
    let _subscription = cx.update(|cx| {
        cx.subscribe(&f.host, move |_, response: &Response, _| {
            seen.borrow_mut().push(response.clone());
        })
    });
    let (id, receiver) = request(&mut f, cx);
    reply(
        &f,
        format!(
            r#"{{"t":"pluginprogress","id":{id},"message":{{"op":"progress","message":"Working"}}}}"#
        ),
        cx,
    );
    assert!(matches!(
        receiver.try_recv(),
        Err(async_channel::TryRecvError::Empty)
    ));
    f.host.read_with(cx, |host, _| {
        assert!(host.pending.contains_key(&id));
        assert!(host.waiters.contains_key(&id));
    });
    assert_eq!(responses.borrow().len(), 1);
    assert_eq!(
        responses.borrow()[0]
            .message
            .get("op")
            .and_then(Json::as_str),
        Some("progress")
    );
    reply(
        &f,
        format!(
            r#"{{"t":"pluginruntime","id":{id},"ok":false,"code":"operation","error":"Cannot run","message":{{"op":"error","message":"Cannot run"}}}}"#
        ),
        cx,
    );
    let terminal = receiver.try_recv().unwrap();
    assert!(!terminal.ok);
    assert_eq!(terminal.error, "Cannot run");
    assert_eq!(responses.borrow().len(), 2);
    assert!(!responses.borrow()[1].ok);
    assert!(matches!(
        receiver.try_recv(),
        Err(async_channel::TryRecvError::Closed)
    ));
    f.host.read_with(cx, |host, _| {
        assert!(!host.pending.contains_key(&id));
        assert!(!host.waiters.contains_key(&id));
    });
}

#[gpui::test]
fn plugin_host_busy_request_and_operation_errors_keep_ready_answers(cx: &mut TestAppContext) {
    let mut f = fixture(cx);
    for code in ["busy", "request", "operation"] {
        let (id, receiver) = request(&mut f, cx);
        reply(
            &f,
            format!(
                r#"{{"t":"pluginruntime","id":{id},"ok":false,"code":"{code}","error":"Refused"}}"#
            ),
            cx,
        );
        assert!(!receiver.try_recv().unwrap().ok);
        f.host.read_with(cx, |host, _| {
            assert!(host.ready.contains(PLUGIN), "{code}");
            assert!(
                host.answers
                    .contains_key(&(PLUGIN.into(), "targets".into())),
                "{code}"
            );
        });
        assert!(
            (f.lines)().is_none(),
            "{code} must not trigger preparation refresh"
        );
    }
    let (id, receiver) = request(&mut f, cx);
    reply(
        &f,
        format!(r#"{{"t":"pluginruntime","id":{id},"ok":false,"code":"crash","error":"Stopped"}}"#),
        cx,
    );
    assert!(!receiver.try_recv().unwrap().ok);
    f.host.read_with(cx, |host, _| {
        assert!(!host.ready.contains(PLUGIN));
        assert!(host.answers.is_empty());
    });
    assert_eq!(
        (f.lines)(),
        Some(wire::request_line(0, "list", "", Vec::new()))
    );
}

#[gpui::test]
fn plugin_host_pluginop_refreshes_and_listing_retires_cached_answers(cx: &mut TestAppContext) {
    let mut f = fixture(cx);
    reply(&f, r#"{"t":"pluginop","id":4,"op":"enable","plugin":"example.checksums","ok":true,"code":"","detail":"","version":"1.0.0"}"#.into(), cx);
    assert_eq!(
        (f.lines)(),
        Some(wire::request_line(0, "list", "", Vec::new()))
    );
    let (_id, _receiver) = request(&mut f, cx);
    reply(&f, r#"{"t":"plugins","id":0,"plugins":[]}"#.into(), cx);
    f.host.read_with(cx, |host, _| {
        assert!(host.ready.is_empty());
        assert!(host.answers.is_empty());
    });
}

#[gpui::test]
fn plugin_host_failure_closes_waiters_and_emits_closed(cx: &mut TestAppContext) {
    let mut f = fixture(cx);
    let closed = Rc::new(RefCell::new(0));
    let seen = closed.clone();
    let _subscription = cx.update(|cx| {
        cx.subscribe(&f.host, move |_, _: &Closed, _| {
            *seen.borrow_mut() += 1;
        })
    });
    let (_id, receiver) = request(&mut f, cx);
    f.output.try_send(Event::Drained).unwrap();
    cx.run_until_parked();
    assert_eq!(*closed.borrow(), 1);
    assert!(matches!(
        receiver.try_recv(),
        Err(async_channel::TryRecvError::Closed)
    ));
    f.host.read_with(cx, |host, _| {
        assert!(host.ready.is_empty());
        assert!(host.answers.is_empty());
        assert!(host.pending.is_empty());
        assert!(host.waiters.is_empty());
        assert!(host.backend.is_none());
    });
}
