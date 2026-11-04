//! Menu input and readers through the real window host, rather than an isolated menu.
use crate::{backend::Backend, ipc::server::Event, state::tests::shell_with, window::root::Root};
use flea::ipc::wire::{Reply, Request};
use gpui::{AppContext, TestAppContext, VisualTestContext, WindowHandle};

pub(super) struct Fixture {
    pub handle: WindowHandle<Root>,
    pub requests: Box<dyn FnMut() -> Option<String>>,
}

pub(super) fn open(cx: &mut TestAppContext) -> Fixture {
    let (backend, mut receive) = Backend::test_lines();
    // Observe this owner's real requests while the joined shell also asks status/list producers.
    let receive = move || loop {
        let line = receive()?;
        let document = flea::jsondoc::parse(&line).unwrap();
        if matches!(
            document.get("c").and_then(flea::jsondoc::Json::as_str),
            Some("menuaction" | "actions")
        ) {
            return Some(line);
        }
    };
    let handle = cx.add_window(|window, cx| {
        let shell = cx.new(|cx| shell_with(backend, cx));
        shell.update(cx, |shell, cx| window.focus(&shell.focus, cx));
        Root { shell }
    });
    Fixture {
        handle,
        requests: Box::new(receive),
    }
}

pub(super) fn read(cx: &mut TestAppContext, handle: WindowHandle<Root>, function: &str) -> Reply {
    read_args(cx, handle, function, &[])
}

pub(super) fn read_args(
    cx: &mut TestAppContext,
    handle: WindowHandle<Root>,
    function: &str,
    arguments: &[&str],
) -> Reply {
    let (answer, receive) = std::sync::mpsc::sync_channel(1);
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| {
                shell.ipc(
                    Event::Call {
                        request: Request::Call {
                            target: "flea".into(),
                            function: function.into(),
                            arguments: arguments.iter().map(|arg| (*arg).into()).collect(),
                        },
                        answer,
                    },
                    window,
                    cx,
                )
            });
        })
        .unwrap();
    receive
        .try_recv()
        .expect("the live window must answer")
        .reply
}

pub(super) fn receive(cx: &mut TestAppContext, handle: WindowHandle<Root>, line: &str) {
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| {
                shell.receive(0, crate::backend::Event::Producer(line.into()), window, cx)
            })
        })
        .unwrap();
}

pub(super) fn ready(cx: &mut TestAppContext, fixture: &mut Fixture) -> u64 {
    let line = loop {
        let line = (fixture.requests)().expect("the menu requests a snapshot");
        if line.contains("\"op\":\"snapshot\"") {
            break line;
        }
        assert!(line.contains("\"op\":\"close\""), "{line}");
    };
    let id = flea::json::field_usize(&line, "id").unwrap() as u64;
    receive(cx, fixture.handle, &format!("{{\"t\":\"menuaction\",\"id\":{id},\"op\":\"snapshot\",\"ok\":true,\"kinds\":[32768]}}"));
    let apps = (fixture.requests)().expect("the menu asks its application producer");
    assert!(apps.contains("\"op\":\"applications\""), "{apps}");
    receive(cx, fixture.handle, &format!("{{\"t\":\"menuaction\",\"id\":{id},\"op\":\"applications\",\"ok\":true,\"applications\":[{{\"id\":\"text.desktop\",\"label\":\"Text editor\",\"icon\":\"text\",\"default\":true}}]}}"));
    id
}

#[gpui::test]
fn u06_live_menu_keyboard_mount_and_dismiss(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    let handle = fixture.handle;
    let closed = read(cx, handle, "contextMenuVisible");
    assert_eq!(
        (closed.code, closed.stdout.as_str()),
        (0, "false\n"),
        "the menu reader is registered on the live window"
    );
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_keystrokes("m");
    assert_eq!(read(cx, handle, "contextMenuVisible").stdout, "false\n");
    ready(cx, &mut fixture);
    let shown = read(cx, handle, "contextMenuVisible");
    assert_eq!(shown.stdout, "true\n", "the menu key mounts the menu");
    assert!(read(cx, handle, "contextMenuEntries")
        .stdout
        .starts_with("Open|"));
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_keystrokes("escape");
    assert_eq!(read(cx, handle, "contextMenuVisible").stdout, "false\n");
}

#[gpui::test]
fn u06_live_menu_model_and_snapshot_state(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    let handle = fixture.handle;
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_keystrokes("m");
    assert_eq!(read(cx, handle, "contextMenuModel").stdout.trim(), "[]");
    assert_eq!(read(cx, handle, "contextMenuRect").stdout, "\n");
    let state = flea::jsondoc::parse(read(cx, handle, "menuState").stdout.trim()).unwrap();
    assert_eq!(
        state
            .get("snapshotReady")
            .and_then(flea::jsondoc::Json::as_bool),
        Some(false)
    );
    ready(cx, &mut fixture);
    let reply = read(cx, handle, "contextMenuModel");
    assert_eq!(reply.code, 0, "the actual drawn menu inventory is readable");
    let model = flea::jsondoc::parse(reply.stdout.trim()).unwrap();
    assert!(model
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row.get("action").and_then(flea::jsondoc::Json::as_str) == Some("rename")));
    let state = flea::jsondoc::parse(read(cx, handle, "menuState").stdout.trim()).unwrap();
    assert_eq!(
        state
            .get("snapshotReady")
            .and_then(flea::jsondoc::Json::as_bool),
        Some(true)
    );
    assert_eq!(
        state.get("atomic"),
        Some(&flea::jsondoc::Json::Null),
        "the test platform cannot fabricate compositor feedback"
    );
}

#[gpui::test]
fn u06_live_menu_readers_keep_unknown_capabilities_refused(cx: &mut TestAppContext) {
    let fixture = open(cx);
    let handle = fixture.handle;
    for function in [
        "contextMenuCursor",
        "contextMenuEntries",
        "contextMenuGlyphs",
        "contextMenuHints",
        "contextMenuModel",
        "contextMenuRect",
        "contextMenuSubmenuEntries",
        "contextMenuSubmenuGlyphs",
        "contextMenuVisible",
        "menuState",
    ] {
        assert_eq!(read(cx, handle, function).code, 0, "{function}");
    }
    for function in ["contextMenuRowCentre", "contextMenuRowProbe"] {
        assert_eq!(
            read_args(cx, handle, function, &["-1"]).stdout,
            "\n",
            "an absent row has no geometry"
        );
    }
    assert_eq!(
        read(cx, handle, "contextMenuFocusState").code,
        0,
        "with no menu open the focus reader answers its idle shape"
    );
}
