use crate::{backend::Backend, ipc::server::Event, state::tests::shell_with};
use flea::ipc::wire::Request;
use gpui::{AppContext, TestAppContext};

#[gpui::test]
fn u32_trash_readers_are_registered_on_the_live_window(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let window = cx.add_empty_window();
    let shell = window.update(|_, cx| cx.new(|cx| shell_with(backend, cx)));
    let (answer, reply) = std::sync::mpsc::sync_channel(1);
    window.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.ipc(
                Event::Call {
                    request: Request::Call {
                        target: "flea".into(),
                        function: "trashState".into(),
                        arguments: vec![],
                    },
                    answer,
                },
                window,
                cx,
            );
        });
    });
    assert_eq!(
        reply.try_recv().unwrap().reply.code,
        0,
        "the native Trash state reader is mounted"
    );
}

use crate::{
    backend::Event as BackendEvent, list::bench::real_cx, state::Shell, window::root::Root,
};
use flea::jsondoc::{self, Json};
use gpui::{point, px, Context, Modifiers, VisualTestContext, Window, WindowHandle};

pub(super) fn root(cx: &mut TestAppContext) -> WindowHandle<Root> {
    let (backend, requests) = Backend::test_channel();
    cx.add_window(|_, cx| {
        let shell = cx.new(|cx| shell_with(backend, cx));
        cx.observe(&shell, move |_, _, _| {
            let _ = &requests;
        })
        .detach();
        Root { shell }
    })
}

pub(super) fn shell<R>(
    cx: &mut TestAppContext,
    window: WindowHandle<Root>,
    f: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>) -> R,
) -> R {
    window
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| f(shell, window, cx))
        })
        .unwrap()
}

pub(super) fn call(
    cx: &mut TestAppContext,
    window: WindowHandle<Root>,
    name: &str,
    arguments: &[&str],
) -> String {
    shell(cx, window, |shell, window, cx| {
        let (answer, reply) = std::sync::mpsc::sync_channel(1);
        shell.ipc(
            Event::Call {
                request: Request::Call {
                    target: "flea".into(),
                    function: name.into(),
                    arguments: arguments.iter().map(|arg| (*arg).into()).collect(),
                },
                answer,
            },
            window,
            cx,
        );
        let reply = reply.try_recv().unwrap().reply;
        assert_eq!(reply.code, 0, "{reply:?}");
        reply.stdout.trim_end().into()
    })
}

pub(super) fn state(cx: &mut TestAppContext, window: WindowHandle<Root>) -> Json {
    jsondoc::parse(&call(cx, window, "trashState", &[])).unwrap()
}

pub(super) fn answer(cx: &mut TestAppContext, window: WindowHandle<Root>, op: &str, fields: &str) {
    shell(cx, window, |shell, window, cx| {
        let host = shell.trash.as_ref().unwrap();
        let id = host.pane.read(cx).view().request_id;
        let line = format!(r#"{{"t":"trashbrowse","id":{id},"op":"{op}","ok":true{fields}}}"#);
        shell.receive(host.side, BackendEvent::TrashLine(line), window, cx);
        shell.flush_trash(window, cx);
    });
}

pub(super) fn opened(cx: &mut TestAppContext, window: WindowHandle<Root>) {
    shell(cx, window, |shell, window, cx| {
        shell.mount_trash(window, cx);
        shell
            .trash
            .as_ref()
            .unwrap()
            .pane
            .update(cx, |pane, _| pane.monitor_start = None);
        assert!(shell.trash_action("openTrash", window, cx));
    });
    answer(
        cx,
        window,
        "list",
        r#", "total":2,"start":0,"present":["trash:///alpha","trash:///beta"],"rows":[{"uri":"trash:///alpha","original":"/private/alpha.txt","identity":"alpha-identity","size":6},{"uri":"trash:///beta","original":"/private/beta.txt","identity":"beta-identity","size":5}]"#,
    );
    answer(cx, window, "summary", r#", "bytes":11,"partial":false"#);
    draw(cx, window);
}

pub(super) fn draw(cx: &mut TestAppContext, window: WindowHandle<Root>) {
    cx.update_window(window.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
}

#[test]
fn u32_native_row_click_arm_other_key_and_cancel_keep_identities_and_counts() {
    let mut cx = real_cx();
    let window = root(&mut cx);
    opened(&mut cx, window);
    let centre = call(&mut cx, window, "trashRowCentre", &["0"]);
    let coords: Vec<f32> = centre
        .split_whitespace()
        .map(|n| n.parse().unwrap())
        .collect();
    assert_eq!(
        coords.len(),
        2,
        "a painted row supplies its window coordinates"
    );
    let mut visual = VisualTestContext::from_window(window.into(), &cx);
    visual.simulate_click(point(px(coords[0]), px(coords[1])), Modifiers::none());
    let selected = state(&mut cx, window);
    assert_eq!(
        selected.get("selectedCount").and_then(Json::as_f64),
        Some(1.)
    );
    assert_eq!(
        selected.get("rows").unwrap().as_array().unwrap()[0].get("selected"),
        Some(&Json::Bool(true))
    );
    cx.simulate_keystrokes(window.into(), "d");
    let prompt = call(&mut cx, window, "lastMessage", &[]);
    assert_eq!(
        prompt,
        "Press d again to review permanent deletion, or Delete on its own."
    );
    cx.simulate_keystrokes(window.into(), "j");
    assert_ne!(
        call(&mut cx, window, "lastMessage", &[]),
        prompt,
        "a different key ends the prompt"
    );
    cx.simulate_keystrokes(window.into(), "k d d");
    shell(&mut cx, window, |shell, _, cx| {
        let pane = shell.trash.as_ref().unwrap().pane.read(cx);
        assert_eq!(pane.view().pending_op, "prepare");
        assert_eq!(pane.view().selection.uris(), ["trash:///alpha"]);
        assert_eq!(
            pane.view().selection.identity("trash:///alpha"),
            "alpha-identity"
        );
    });
    answer(
        &mut cx,
        window,
        "prepare",
        r#", "token":71,"count":1,"bytes":6,"all":false"#,
    );
    draw(&mut cx, window);
    let shown = state(&mut cx, window);
    assert_eq!(
        shown.get("confirmation").unwrap().get("opened"),
        Some(&Json::Bool(true))
    );
    assert_eq!(
        shown
            .get("confirmation")
            .unwrap()
            .get("count")
            .and_then(Json::as_f64),
        Some(1.)
    );
    let mut visual = VisualTestContext::from_window(window.into(), &cx);
    assert!(
        visual.debug_bounds("op1-card").is_some(),
        "the real op1 confirmation is drawn"
    );
    cx.simulate_keystrokes(window.into(), "enter");
    cx.run_until_parked();
    answer(&mut cx, window, "cancel", "");
    let kept = state(&mut cx, window);
    assert_eq!(kept.get("total").and_then(Json::as_f64), Some(2.));
    assert_eq!(kept.get("count").and_then(Json::as_f64), Some(2.));
    assert_eq!(
        kept.get("confirmation").unwrap().get("opened"),
        Some(&Json::Bool(false))
    );
    cx.simulate_keystrokes(window.into(), "j");
    assert_eq!(
        state(&mut cx, window).get("cursor").and_then(Json::as_f64),
        Some(0.),
        "Cancel's refresh holds row actions until the replacement commits"
    );
    answer(
        &mut cx,
        window,
        "list",
        r#", "total":2,"start":0,"present":["trash:///alpha","trash:///beta"],"rows":[{"uri":"trash:///alpha","original":"/private/alpha.txt","identity":"alpha-identity","size":6},{"uri":"trash:///beta","original":"/private/beta.txt","identity":"beta-identity","size":5}]"#,
    );
    answer(
        &mut cx,
        window,
        "summary",
        r#", "bytes":11,"partial":false"#,
    );
    draw(&mut cx, window);
    cx.simulate_keystrokes(window.into(), "j");
    assert_eq!(
        state(&mut cx, window).get("cursor").and_then(Json::as_f64),
        Some(1.),
        "Cancel returns keyboard input to Trash after its complete refresh"
    );
}

#[test]
fn u32_unpainted_rows_stale_tokens_and_failed_producer_do_not_act() {
    let mut cx = real_cx();
    let window = root(&mut cx);
    assert!(call(&mut cx, window, "trashRowCentre", &["0"]).is_empty());
    opened(&mut cx, window);
    assert!(call(&mut cx, window, "trashRowCentre", &["2"]).is_empty());
    assert!(call(&mut cx, window, "trashRowCentre", &["-1"]).is_empty());
    cx.simulate_keystrokes(window.into(), "d d");
    answer(
        &mut cx,
        window,
        "prepare",
        r#", "token":41,"count":1,"bytes":6,"all":false"#,
    );
    shell(&mut cx, window, |shell, _, cx| {
        shell
            .trash
            .as_ref()
            .unwrap()
            .pane
            .update(cx, |pane, cx| pane.card_answered(Some(40), cx));
        assert_eq!(
            shell
                .trash
                .as_ref()
                .unwrap()
                .pane
                .read(cx)
                .view()
                .confirmation
                .as_ref()
                .unwrap()
                .token,
            41
        );
    });
    cx.simulate_keystrokes(window.into(), "escape");
    cx.run_until_parked();
    answer(&mut cx, window, "cancel", "");
    shell(&mut cx, window, |shell, window, cx| {
        shell
            .trash
            .as_ref()
            .unwrap()
            .pane
            .update(cx, |pane, cx| pane.action("reload", false, cx));
        shell.receive(
            0,
            BackendEvent::Failed(crate::backend::Failure {
                key: "window.backend_stopped",
                detail: String::new(),
            }),
            window,
            cx,
        );
        shell.flush_trash(window, cx);
    });
    let failed = state(&mut cx, window);
    assert_eq!(failed.get("busy"), Some(&Json::Bool(false)));
    assert_eq!(
        failed.get("total").and_then(Json::as_f64),
        Some(2.),
        "a failed refresh retains its rows"
    );
    shell(&mut cx, window, |shell, _, cx| {
        assert!(shell
            .trash
            .as_ref()
            .unwrap()
            .pane
            .read(cx)
            .view()
            .error_text
            .is_some())
    });
}

#[test]
fn u32_menu_refuses_a_selection_changed_since_it_opened() {
    let mut cx = real_cx();
    let window = root(&mut cx);
    opened(&mut cx, window);
    cx.simulate_keystrokes(window.into(), "m");
    assert!(call(&mut cx, window, "contextMenuEntries", &[]).contains("Restore"));
    shell(&mut cx, window, |shell, _, cx| {
        shell
            .trash
            .as_ref()
            .unwrap()
            .pane
            .update(cx, |pane, cx| pane.click_row(1, false, cx));
    });
    cx.simulate_keystrokes(window.into(), "enter");
    let snapshot = state(&mut cx, window);
    assert_eq!(
        snapshot.get("busy"),
        Some(&Json::Bool(false)),
        "the stale Restore did not reach the producer"
    );
    assert_eq!(
        snapshot.get("rows").unwrap().as_array().unwrap()[1].get("selected"),
        Some(&Json::Bool(true))
    );
}
