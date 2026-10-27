//! U13 exercises readers through the live shell and its painted operation card.
use crate::{
    backend::Backend,
    ipc::server::Event,
    state::{tests::shell_with, Shell},
    window::root::Root,
};
use flea::ipc::wire::{Reply, Request};
use gpui::{AppContext, TestAppContext, Window, WindowHandle};

// The receiver stays with the caller, so the card's own collision request reaches a live backend.
fn open(cx: &mut TestAppContext) -> (WindowHandle<Root>, impl FnMut() -> Option<(usize, usize)>) {
    let (backend, receive) = Backend::test_channel();
    let handle = cx.add_window(|_, cx| Root {
        shell: cx.new(|cx| shell_with(backend, cx)),
    });
    (handle, receive)
}

fn shell<R>(
    cx: &mut TestAppContext,
    handle: WindowHandle<Root>,
    update: impl FnOnce(&mut Shell, &mut Window, &mut gpui::Context<Shell>) -> R,
) -> R {
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| update(shell, window, cx))
        })
        .unwrap()
}

fn call(cx: &mut TestAppContext, handle: WindowHandle<Root>, function: &str) -> Reply {
    let (answer, receive) = std::sync::mpsc::sync_channel(1);
    shell(cx, handle, |shell, window, cx| {
        shell.ipc(
            Event::Call {
                request: Request::Call {
                    target: "flea".into(),
                    function: function.into(),
                    arguments: vec![],
                },
                answer,
            },
            window,
            cx,
        )
    });
    receive.try_recv().unwrap().reply
}

#[gpui::test]
fn u13_live_collision_reader_answers_before_any_card(cx: &mut TestAppContext) {
    let (handle, _backend) = open(cx);
    let reply = call(cx, handle, "collideState");
    assert_eq!(reply.code, 0, "a closed collision card is observable");
    let state = flea::jsondoc::parse(reply.stdout.trim()).unwrap();
    assert_eq!(
        state.get("opened").and_then(flea::jsondoc::Json::as_bool),
        Some(false)
    );
    assert!(
        !shell(cx, handle, |shell, _, _| shell.overlay.is_some()),
        "reading must not instantiate a card"
    );
}

#[gpui::test]
fn u13_live_collision_reader_reports_the_drawn_card(cx: &mut TestAppContext) {
    let (handle, _backend) = open(cx);
    shell(cx, handle, |shell, window, cx| {
        shell.open_overlay_copy_to(false, "/files", window, cx);
        let overlay = shell.overlay.clone().unwrap();
        overlay.update(cx, |overlay, cx| {
            let catalog = overlay.catalog.clone();
            overlay.model.ask(&catalog, &super::tests::request(), &[], false);
            overlay.line(r#"{"t":"collisions","id":1,"total":1,"names":[{"n":"notes.txt","d":false,"i":""}],"mergeable":false}"#, cx);
        });
    });
    cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
    let reply = call(cx, handle, "collideState");
    assert_eq!(
        reply.code, 0,
        "the live dispatcher must register the operation reader"
    );
    let state = flea::jsondoc::parse(reply.stdout.trim()).unwrap();
    assert_eq!(
        state.get("focus").and_then(flea::jsondoc::Json::as_str),
        Some("keep")
    );
    assert_eq!(
        state.get("explainLines"),
        Some(&flea::jsondoc::Json::Num("1".into()))
    );
    assert_eq!(
        state
            .get("buttonsFit")
            .and_then(flea::jsondoc::Json::as_bool),
        Some(true)
    );
}

#[gpui::test]
fn u13_ipc_closed_operations_never_impersonate_the_network_form(cx: &mut TestAppContext) {
    let (handle, _backend) = open(cx);
    shell(cx, handle, |shell, window, cx| {
        shell.open_overlay_copy_to(false, "/files", window, cx)
    });
    assert_eq!(
        call(cx, handle, "dialogOpen"),
        Reply::value("false"),
        "an operation card does not open the network form"
    );
}

#[gpui::test]
fn u13_collision_measurements_detect_real_clipping(cx: &mut TestAppContext) {
    let (handle, _backend) = open(cx);
    shell(cx, handle, |shell, window, cx| {
        shell.open_overlay_copy_to(false, "/files", window, cx);
        shell.overlay.clone().unwrap().update(cx, |overlay, cx| {
            let catalog = overlay.catalog.clone();
            overlay.model.ask(&catalog, &super::tests::request(), &[], false);
            let name = "long-name".repeat(40);
            overlay.line(&format!(r#"{{"t":"collisions","id":1,"total":1,"names":[{{"n":"{name}","d":false,"i":""}}],"mergeable":false}}"#), cx);
        });
    });
    let visual = gpui::VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_resize(gpui::size(gpui::px(280.), gpui::px(400.)));
    cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
    let reply = call(cx, handle, "collideState");
    let state = flea::jsondoc::parse(reply.stdout.trim()).unwrap();
    assert_eq!(
        state
            .get("titleTruncated")
            .and_then(flea::jsondoc::Json::as_bool),
        Some(true)
    );
    assert_eq!(
        state
            .get("buttonsFit")
            .and_then(flea::jsondoc::Json::as_bool),
        Some(false)
    );
}

pub(super) fn next(
    events: &async_channel::Receiver<crate::backend::Event>,
) -> crate::backend::Event {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if let Ok(event) = events.try_recv() {
            return event;
        }
        assert!(std::time::Instant::now() < until, "backend reply timed out");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[gpui::test]
fn u13_pipe_collision_keeps_identity_and_partial_completion(cx: &mut TestAppContext) {
    let mut command = std::process::Command::new("python3");
    command.args(["-u", "-c", r#"
import json,sys
for line in sys.stdin:
    request=json.loads(line)
    if request['c']=='quit': break
    if request['c']=='collisions':
        for id in [request['id']+1,request['id']]:
            print(json.dumps(dict(t='collisions',id=id,total=1,names=[dict(n='notes.txt',d=False,i='')],mergeable=False)),flush=True)
    if request['c']=='transfer':
        print('{"t":"transferstarted","id":9,"n":2,"move":false}',flush=True)
        print('{"t":"transferitem","id":9,"index":0,"name":"notes.txt","ok":false,"err":"read refused"}',flush=True)
        print('{"t":"transferdone","id":9,"ok":1,"failed":1}',flush=True)
"#]);
    let backend = Backend::start_command(command, "/files".into(), None, 320).unwrap();
    let events = backend.replies.clone();
    let handle = cx.add_window(|window, cx| {
        let shell = cx.new(|cx| shell_with(backend, cx));
        let focus = shell.read(cx).focus.clone();
        window.focus(&focus, cx);
        Root { shell }
    });
    shell(cx, handle, |shell, window, cx| {
        assert!(shell.ask_operation(0, &super::tests::request(), &[], false, window, cx))
    });
    shell(cx, handle, |shell, window, cx| {
        shell.receive(0, next(&events), window, cx)
    });
    assert_eq!(
        call(cx, handle, "collideState").stdout.trim(),
        r#"{"opened":false}"#
    );
    shell(cx, handle, |shell, window, cx| {
        shell.receive(0, next(&events), window, cx)
    });
    cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
    let mut visual = gpui::VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_keystrokes("ctrl-enter");
    let before = call(cx, handle, "collideState");
    assert!(
        before.stdout.contains("\"opened\": true"),
        "modified Enter must keep the card open: {before:?}"
    );
    let mut visual = gpui::VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_keystrokes("enter");
    for _ in 0..3 {
        shell(cx, handle, |shell, window, cx| {
            shell.receive(0, next(&events), window, cx)
        });
    }
    shell(cx, handle, |shell, _, cx| {
        let view = shell.overlay.as_ref().unwrap().read(cx);
        assert!(!view.model.wire.transfer.running);
        assert_eq!(
            shell.panes[0].as_ref().unwrap().status.as_ref(),
            "Copied 1 of 2 · 1 failed · z undoes"
        );
    });
}
