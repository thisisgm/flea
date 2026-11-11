//! A real window and text system over bounded backend fixture replies, with no filesystem mutation.
use crate::{
    backend::{Backend, Event},
    state::{tests::shell_with, Shell},
    window::startup::View,
};
use flea::{
    jsondoc::Json,
    protocol::{Reply, RowFacts},
};
use gpui::{px, size, AppContext, Context, TestAppContext, TestDispatcher, Window, WindowHandle};
use std::time::Duration;

pub fn context() -> TestAppContext {
    TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        gpui_platform::current_platform(true).text_system(),
    )
}

pub fn open(
    cx: &mut TestAppContext,
    path: &str,
) -> (WindowHandle<Shell>, impl FnMut() -> Option<(usize, usize)>) {
    open_at(cx, path, 1400., 3)
}

pub fn open_at(
    cx: &mut TestAppContext,
    path: &str,
    width: f32,
    count: usize,
) -> (WindowHandle<Shell>, impl FnMut() -> Option<(usize, usize)>) {
    let (backend, requests) = Backend::test_channel();
    let window = cx.open_window(size(px(width), px(700.)), |window, cx| {
        let mut shell = shell_with(backend, cx);
        shell.launch.settings.columns = count;
        let host = shell.panes[0].as_mut().unwrap();
        host.pane = crate::pane::Pane::new(path.into(), 646.);
        // The fresh pane takes test time, so the entry cap moves only when a test advances it.
        host.pane.clock.use_manual();
        host.pane
            .receive(Reply::Listed {
                path: path.into(),
                total: 2,
            })
            .unwrap();
        host.pane
            .receive(Reply::Rows {
                start: 0,
                listing: 3,
                rows: vec![row("d01", true), row("d02", true)],
            })
            .unwrap();
        host.changed(&shell.locale);
        host.nav
            .after_reply(&mut host.pane, path, &mut host.selected);
        shell.path_labels[0] = path.to_owned().into();
        shell.set_view(View::Columns, window, cx);
        shell
    });
    (window, requests)
}

pub fn row(name: &str, dir: bool) -> RowFacts {
    RowFacts {
        source: Some(format!("1:2:{}", if dir { 0o040000 } else { 0o100000 })),
        name: name.into(),
        dir,
        icon: if dir { "folder" } else { "text-plain" }.into(),
        ..Default::default()
    }
}

/// Moves the shown pane's clock by hand; no executor timer runs during the move.
pub fn advance(cx: &mut TestAppContext, handle: WindowHandle<Shell>, by: Duration) {
    change(cx, handle, |shell, _, _| {
        shell.panes[0]
            .as_ref()
            .unwrap()
            .pane
            .clock
            .advance_manual(by)
    });
}

pub fn change(
    cx: &mut TestAppContext,
    handle: WindowHandle<Shell>,
    change: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>),
) {
    handle.update(cx, change).unwrap();
}

pub fn frame(cx: &mut TestAppContext, handle: WindowHandle<Shell>) {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
}

pub fn document(shell: &Shell, path: &str, rows: &[(&str, bool)], failed: bool) -> Json {
    let first = shell.panes[0].as_ref().unwrap().pane.first_count();
    let folders = shell.panes[0].as_ref().unwrap().nav.folders.borrow();
    Json::Obj(vec![
        ("t".into(), Json::Str("peeked".into())),
        ("path".into(), Json::Str(path.into())),
        ("first".into(), Json::Num(first.to_string())),
        ("n".into(), Json::Num(rows.len().to_string())),
        ("hidden".into(), Json::Bool(folders.hidden)),
        ("hiddenLast".into(), Json::Bool(folders.hidden_last)),
        ("failed".into(), Json::Bool(failed)),
        ("mode".into(), Json::Num("0".into())),
        (
            "rows".into(),
            Json::Arr(
                rows.iter()
                    .map(|(name, dir)| {
                        Json::Obj(vec![
                            ("n".into(), Json::Str((*name).into())),
                            ("d".into(), Json::Bool(*dir)),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
}

pub fn peek(
    shell: &mut Shell,
    path: &str,
    rows: &[(&str, bool)],
    failed: bool,
    window: &mut Window,
    cx: &mut Context<Shell>,
) {
    shell.receive(
        0,
        Event::Peeked(document(shell, path, rows, failed)),
        window,
        cx,
    );
}

/// Answers every drawn column still out with one folder row, which ends the entry hold on the List.
pub fn settle(shell: &mut Shell, window: &mut Window, cx: &mut Context<Shell>) {
    let out: Vec<String> = {
        let host = shell.panes[0].as_ref().unwrap();
        let folders = host.nav.folders.borrow();
        let columns = host.columns.as_ref().unwrap();
        columns
            .area
            .drawn
            .iter()
            .filter(|path| {
                let key =
                    crate::columns::peeks::peek_key(path, folders.hidden, folders.hidden_last);
                columns.area.peeks.rows(&key).is_none()
            })
            .cloned()
            .collect()
    };
    for path in out {
        peek(shell, &path, &[("x", true)], false, window, cx);
    }
}

pub fn call(
    shell: &mut Shell,
    function: &str,
    arguments: &[&str],
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> flea::ipc::wire::Reply {
    let (answer, receive) = std::sync::mpsc::sync_channel(1);
    shell.ipc(
        crate::ipc::server::Event::Call {
            request: flea::ipc::wire::Request::Call {
                target: "flea".into(),
                function: function.into(),
                arguments: arguments.iter().map(|arg| (*arg).into()).collect(),
            },
            answer,
        },
        window,
        cx,
    );
    let reply = receive.try_recv().unwrap().reply;
    assert_eq!(reply.code, 0, "{}", reply.stderr);
    reply
}
