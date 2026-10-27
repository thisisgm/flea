//! Shared setup drives the production Root, menu validation and native control geometry.
use crate::{
    backend::Backend,
    g2::mn1::menu::Outcome,
    state::{tests::shell_with, Shell},
    window::root::Root,
};
use flea::protocol::RowFacts;
use gpui::{AppContext, TestAppContext, VisualTestContext, Window, WindowHandle};
use std::rc::Rc;

pub(super) fn edit<R>(
    cx: &mut TestAppContext,
    handle: WindowHandle<Root>,
    run: impl FnOnce(&mut Shell, &mut Window, &mut gpui::Context<Shell>) -> R,
) -> R {
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| run(shell, window, cx))
        })
        .unwrap()
}

pub(super) fn model(
    cx: &mut TestAppContext,
    handle: WindowHandle<Root>,
) -> crate::g2::op3::permissions::Permissions {
    edit(cx, handle, |shell, _, cx| {
        shell
            .permissions
            .view
            .as_ref()
            .unwrap()
            .read(cx)
            .model
            .clone()
    })
}

pub(super) fn setup(
    cx: &mut TestAppContext,
) -> (WindowHandle<Root>, impl FnMut() -> Option<String>) {
    let (backend, requests) = Backend::test_lines();
    let handle = cx.add_window(|_, cx| Root {
        shell: cx.new(|cx| shell_with(backend, cx)),
    });
    edit(cx, handle, |shell, window, cx| {
        shell.launch.settings.data =
            flea::jsondoc::parse(r#"{"menu":{"hidden":["runScript","actions"]}}"#).unwrap();
        let host = shell.panes[0].as_mut().unwrap();
        host.pane.held = Rc::new(crate::list::pane::Held {
            listing: 2,
            total: 2,
            start: 0,
            rows: ["notes.txt", "other.txt"]
                .into_iter()
                .map(|name| RowFacts {
                    name: name.into(),
                    mode: 0o100644,
                    icon: "text-x-generic".into(),
                    ..Default::default()
                })
                .collect(),
        });
        window.focus(&shell.focus, cx);
    });
    (handle, requests)
}

pub(super) fn receive(cx: &mut TestAppContext, handle: WindowHandle<Root>, line: &str) {
    receive_on(cx, handle, 0, line);
}

pub(super) fn receive_on(
    cx: &mut TestAppContext,
    handle: WindowHandle<Root>,
    side: usize,
    line: &str,
) {
    edit(cx, handle, |shell, window, cx| {
        shell.receive(side, Backend::test_reply_line(line), window, cx)
    });
}

pub(super) fn next(
    requests: &mut impl FnMut() -> Option<String>,
    command: &str,
    op: &str,
) -> String {
    while let Some(line) = requests() {
        let json = flea::jsondoc::parse(&line).unwrap();
        if json.get("c").and_then(flea::jsondoc::Json::as_str) == Some(command)
            && (op.is_empty() || json.get("op").and_then(flea::jsondoc::Json::as_str) == Some(op))
        {
            return line;
        }
    }
    panic!("mounted Permissions did not send {command}/{op}");
}

pub(super) fn choose(
    cx: &mut TestAppContext,
    handle: WindowHandle<Root>,
    requests: &mut impl FnMut() -> Option<String>,
) {
    choose_on(cx, handle, requests, 0);
}

pub(super) fn choose_on(
    cx: &mut TestAppContext,
    handle: WindowHandle<Root>,
    requests: &mut impl FnMut() -> Option<String>,
    side: usize,
) {
    edit(cx, handle, |shell, window, cx| {
        shell.open_menu(side, Some(0), (230., 100.), window, cx)
    });
    let snapshot = next(requests, "menuaction", "snapshot");
    let id = flea::json::field_usize(&snapshot, "id").unwrap();
    receive_on(
        cx,
        handle,
        side,
        &format!(r#"{{"t":"menuaction","id":{id},"op":"snapshot","ok":true,"kinds":[32768]}}"#),
    );
    receive_on(
        cx,
        handle,
        side,
        &format!(
            r#"{{"t":"menuaction","id":{id},"op":"applications","ok":true,"applications":[],"kind":"Plain text document"}}"#
        ),
    );
    edit(cx, handle, |shell, window, cx| {
        shell.menu_outcome(
            Outcome::Fire {
                action: "permissions".into(),
                leaf: String::new(),
            },
            window,
            cx,
        )
    });
    let validate = next(requests, "menuaction", "validate");
    assert!(validate.contains(r#""action":"permissions""#));
    let paths = edit(cx, handle, |shell, _, _| {
        let capture = shell.menu.captured.as_ref().unwrap();
        let host = shell.panes[side].as_ref().unwrap();
        capture
            .rows
            .iter()
            .map(|&at| format!("\"/files/{}\"", host.pane.held.row(at).unwrap().name))
            .collect::<Vec<_>>()
            .join(",")
    });
    receive_on(
        cx,
        handle,
        side,
        &format!(
            r#"{{"t":"menuaction","id":{id},"op":"validate","action":"permissions","ok":true,"paths":[{paths}]}}"#
        ),
    );
    cx.run_until_parked();
}

pub(super) fn inspect(
    cx: &mut TestAppContext,
    handle: WindowHandle<Root>,
    id: u64,
    name: &str,
    mode: &str,
    reason: &str,
) {
    receive(
        cx,
        handle,
        &format!(
            r#"{{"t":"permissions","id":{id},"op":"inspect","ok":true,"path":"/files/{name}","directory":false,"mode":"{mode}","uid":1000,"gid":1000,"owner":"gm","group":"gm","reason":"{reason}"}}"#
        ),
    );
}

pub(super) fn click(cx: &mut TestAppContext, handle: WindowHandle<Root>, selector: &'static str) {
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(700.)));
    visual.run_until_parked();
    let bounds = visual
        .debug_bounds(selector)
        .expect("the live card draws this control");
    let at = bounds.center();
    visual.simulate_mouse_down(at, gpui::MouseButton::Left, gpui::Modifiers::default());
    visual.simulate_mouse_up(at, gpui::MouseButton::Left, gpui::Modifiers::default());
    visual.run_until_parked();
}
