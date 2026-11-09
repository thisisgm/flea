//! The archive and flash host through the production window: menu choices, backend requests, replies and the status line.
use crate::{
    backend::{Backend, Event as BackendEvent},
    g2::context_menus::join::{Captured, Chosen},
    ipc::server::Event,
    state::{tests::shell_with, Shell},
    window::{pane::PaneHost, root::Root, startup::View},
};
use flea::{
    ipc::wire::{Reply, Request},
    jsondoc::Json,
};
use gpui::{AppContext, Context, TestAppContext, VisualTestContext, Window, WindowHandle};
use std::{cell::Cell, rc::Rc};

mod cancel_click;
mod compress_geometry;
mod control_click;
mod extract;
mod fixcards2;
mod flash;
mod flash_look;
mod reader;

type Handle = WindowHandle<Root>;

const ROWS: [&str; 4] = ["photos.zip", "disk.img", "notes.txt", "other.txt"];

fn open(cx: &mut TestAppContext) -> Handle {
    let (backend, _) = Backend::test_channel();
    let handle = cx.add_window(|window, cx| Root {
        shell: cx.new(|cx| {
            let mut shell = shell_with(backend, cx);
            shell.panes[0].as_mut().unwrap().backend = None;
            let mut right = PaneHost::new("/right".into(), None, 620., &shell.locale);
            right.backend = None;
            shell.panes[1] = Some(right);
            shell.lifecycle.attach(1);
            shell.view = View::Dual;
            window.focus(&shell.focus, cx);
            shell
        }),
    });
    edit(cx, handle, |shell, window, cx| {
        for (side, path) in [(0, "/files"), (1, "/right")] {
            if side == 0 {
                let pane = &mut shell.panes[side].as_mut().unwrap().pane;
                assert!(pane.begin(path.into(), Default::default(), pane.now()));
            }
            for reply in [
                flea::protocol::Reply::Listed {
                    path: path.into(),
                    total: ROWS.len(),
                },
                flea::protocol::Reply::Rows {
                    start: 0,
                    listing: 3,
                    rows: ROWS
                        .iter()
                        .map(|name| flea::protocol::RowFacts {
                            name: (*name).into(),
                            ..Default::default()
                        })
                        .collect(),
                },
            ] {
                shell.receive(side, BackendEvent::Reply(reply), window, cx);
            }
        }
    });
    frame(cx, handle);
    handle
}

fn edit<R>(
    cx: &mut TestAppContext,
    handle: Handle,
    change: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>) -> R,
) -> R {
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| change(shell, window, cx))
        })
        .unwrap()
}

fn frame(cx: &mut TestAppContext, handle: Handle) {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
}

fn receive(cx: &mut TestAppContext, handle: Handle, side: usize, line: &str) {
    edit(cx, handle, |shell, window, cx| {
        shell.receive(side, Backend::test_reply_line(line), window, cx)
    });
    frame(cx, handle);
}

fn keys(cx: &mut TestAppContext, handle: Handle, keys: &str) {
    for key in keys.split_whitespace() {
        let mut visual = VisualTestContext::from_window(handle.into(), cx);
        if let Some((_, text)) = key.split_once("->") {
            visual.simulate_input(text);
        } else {
            visual.simulate_keystrokes(key);
        }
    }
    frame(cx, handle);
}

fn drain(requests: &mut impl FnMut() -> Option<String>) -> Vec<String> {
    std::iter::from_fn(requests).collect()
}

fn menu_request(requests: &mut impl FnMut() -> Option<String>, operation: &str) -> u64 {
    while let Some(line) = requests() {
        let document = flea::jsondoc::parse(&line).unwrap();
        if document.get("c").and_then(Json::as_str) == Some("menuaction")
            && document.get("op").and_then(Json::as_str) == Some(operation)
        {
            return flea::json::field_usize(&line, "id").unwrap() as u64;
        }
    }
    panic!("the mounted menu did not request {operation}");
}

fn call(cx: &mut TestAppContext, handle: Handle, function: &str) -> String {
    edit(cx, handle, |shell, window, cx| {
        let (answer, receive) = std::sync::mpsc::sync_channel(1);
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
        );
        let reply: Reply = receive.try_recv().unwrap().reply;
        reply.stdout
    })
}

fn model(cx: &mut TestAppContext, handle: Handle) -> Json {
    flea::jsondoc::parse(call(cx, handle, "contextMenuModel").trim()).unwrap()
}

fn footer(cx: &mut TestAppContext, handle: Handle) -> String {
    call(cx, handle, "statusFooterState")
}

fn choose(
    cx: &mut TestAppContext,
    handle: Handle,
    id: u64,
    action: &str,
    leaf: &str,
    paths: &[&str],
) -> Rc<Cell<bool>> {
    let chosen = chosen(id, action, leaf, paths);
    let claimed = chosen.claimed.clone();
    edit(cx, handle, |_, _, cx| cx.emit(chosen));
    frame(cx, handle);
    claimed
}

fn chosen(id: u64, action: &str, leaf: &str, paths: &[&str]) -> Chosen {
    Chosen {
        captured: Captured {
            side: 0,
            listing: 3,
            rows: vec![0],
            cursor: 0,
            path: "/files".into(),
            identity: String::new(),
            id,
            ready: true,
            kinds: Vec::new(),
            background: false,
            target: None,
        },
        action: action.into(),
        leaf: leaf.into(),
        paths: paths.iter().map(|path| (*path).to_owned()).collect(),
        claimed: Rc::new(Cell::new(false)),
    }
}

/// The production window with a test backend on each side and a chooser that answers one image.
fn start(
    cx: &mut TestAppContext,
) -> (
    Handle,
    impl FnMut() -> Option<String>,
    impl FnMut() -> Option<String>,
) {
    let handle = open(cx);
    let (backend, mut requests) = Backend::test_lines();
    let (other, mut other_requests) = Backend::test_lines();
    edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_mut().unwrap().backend = Some(backend);
        shell.panes[1].as_mut().unwrap().backend = Some(other);
        shell.operations.archive.chooser = || {
            let mut command = std::process::Command::new("printf");
            command.arg("/files/chosen.iso\n");
            command
        };
    });
    // The first frame's own requests, such as fsinfo, go before the test reads the wire.
    frame(cx, handle);
    drain(&mut requests);
    drain(&mut other_requests);
    (handle, requests, other_requests)
}

#[test]
fn m9_integ_archive_effects_reach_origin() {
    let mut cx = crate::list::bench::real_cx();
    let (handle, mut requests, mut other_requests) = start(&mut cx);

    // A started backend is asked its formats once; the real menu over the archive row then offers Compress and Extract.
    edit(&mut cx, handle, |shell, _, cx| {
        shell.archive_ask_formats(0, cx);
        shell.archive_ask_formats(0, cx);
    });
    assert_eq!(drain(&mut requests), [r#"{"c":"formats","id":1}"#]);
    keys(&mut cx, handle, "m");
    let opened = drain(&mut requests);
    let snapshot = opened
        .iter()
        .find(|line| line.contains(r#""op":"snapshot""#))
        .map(|line| flea::json::field_usize(line, "id").unwrap())
        .expect("the menu snapshot went out");
    receive(
        &mut cx,
        handle,
        0,
        r#"{"t":"formats","archive":["zip","tar","7z"],"convert":false,"extract":{"archive":true,"sevenZip":true,"zip":true}}"#,
    );
    receive(
        &mut cx,
        handle,
        0,
        &format!(
            r#"{{"t":"menuaction","id":{snapshot},"op":"snapshot","ok":true,"kinds":[32768]}}"#
        ),
    );
    assert_eq!(menu_request(&mut requests, "applications"), snapshot as u64);
    receive(
        &mut cx,
        handle,
        0,
        &format!(
            r#"{{"t":"menuaction","id":{snapshot},"op":"applications","ok":true,"applications":[]}}"#
        ),
    );
    let rows = model(&mut cx, handle);
    let actions: Vec<&str> = rows
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|row| row.get("action").and_then(Json::as_str))
        .collect();
    assert!(
        actions.contains(&"compress") && actions.contains(&"extract"),
        "archive rows reach the menu: {actions:?}"
    );
    keys(&mut cx, handle, "escape");
    drain(&mut requests);
    let before = drain(&mut other_requests);
    assert!(
        before
            .iter()
            .all(|line| !line.contains(r#""c":"archive""#) && !line.contains(r#""c":"formats""#)),
        "{before:?}"
    );

    // A format compress sends the archive request to the origin backend and releases its menu session.
    let claimed = choose(
        &mut cx,
        handle,
        41,
        "compress",
        "zip",
        &["/files/notes.txt"],
    );
    assert!(claimed.get(), "the host claims its choice");
    assert_eq!(
        drain(&mut requests),
        [
            r#"{"c":"archive","op":"compress","paths":["/files/notes.txt"],"dest":"/files/notes.zip","format":"zip","menuId":41}"#,
            r#"{"c":"menuaction","op":"close","id":41}"#,
        ]
    );
    let leaked = drain(&mut other_requests);
    assert!(
        leaked.is_empty(),
        "another pane's backend never takes the request: {leaked:?}"
    );

    // With Password opens the card, keys reach it, the secret leaves only on the wire, and the card closes clean.
    choose(
        &mut cx,
        handle,
        42,
        "compress",
        "compressPassword",
        &["/files/notes.txt"],
    );
    assert!(
        edit(&mut cx, handle, |shell, _, _| shell
            .operations
            .archive
            .card_open()),
        "the compress card opens"
    );
    assert!(
        VisualTestContext::from_window(handle.into(), &cx)
            .debug_bounds("op2-card")
            .is_some(),
        "the card is drawn"
    );
    keys(&mut cx, handle, "p->p w->w");
    keys(&mut cx, handle, "tab");
    keys(&mut cx, handle, "p->p w->w");
    keys(&mut cx, handle, "enter");
    let sent = drain(&mut requests);
    assert_eq!(sent.len(), 2, "{sent:?}");
    assert!(sent[0].starts_with(r#"{"c":"archive","op":"compress","paths":["/files/notes.txt"],"dest":"/files/notes.7z","format":"7z","password":"pw","encryptNames":true,"menuId":42"#), "{}", sent[0]);
    assert_eq!(sent[1], r#"{"c":"menuaction","op":"close","id":42}"#);
    assert!(edit(&mut cx, handle, |shell, _, _| !shell
        .operations
        .archive
        .showing()
        && shell.operations.archive.compress.is_none()));
}

#[test]
fn m9_integ_archive_claims_nothing_it_does_not_own() {
    let mut cx = crate::list::bench::real_cx();
    let handle = open(&mut cx);
    let (backend, mut requests) = Backend::test_lines();
    edit(&mut cx, handle, |shell, _, _| {
        shell.panes[0].as_mut().unwrap().backend = Some(backend)
    });
    let claimed = choose(&mut cx, handle, 50, "copyAs", "path", &["/files/notes.txt"]);
    assert!(
        !claimed.get(),
        "the archive host never claims another producer's choice"
    );
    assert!(drain(&mut requests)
        .iter()
        .all(|line| !line.contains(r#""c":"archive""#)));
    assert!(edit(&mut cx, handle, |shell, _, _| !shell
        .operations
        .archive
        .showing()));
    // A backend that stops ends the card it owned and forgets the formats it reported.
    choose(
        &mut cx,
        handle,
        51,
        "compress",
        "compressPassword",
        &["/files/notes.txt"],
    );
    assert!(edit(&mut cx, handle, |shell, _, cx| {
        shell.archive_ask_formats(0, cx);
        shell.operations.archive.card_open() && shell.operations.archive.formats[0].asked
    }));
    edit(&mut cx, handle, |shell, window, cx| {
        shell.receive(0, BackendEvent::Drained, window, cx)
    });
    edit(&mut cx, handle, |shell, _, _| {
        assert!(
            !shell.operations.archive.card_open(),
            "the stopped backend's card closes"
        );
        assert!(
            !shell.operations.archive.formats[0].asked,
            "a restarted backend is asked again"
        );
    });
}
