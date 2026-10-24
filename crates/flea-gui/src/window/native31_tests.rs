//! Real transport replies reach both mounted consumers; registered geometry observes real tracks.
use super::{pane::PaneHost, root::Root, startup::View};
use crate::{
    backend::Backend,
    state::{tests::shell_with, Shell},
};
use flea::{
    ipc::wire::{Reply, Request},
    jsondoc::{self, Json},
};
use gpui::{AppContext, Context, TestAppContext, Window, WindowHandle};

#[path = "native31_columns_tests.rs"]
mod columns;
#[path = "native31_filesystem_tests.rs"]
mod filesystem;

type Handle = WindowHandle<Root>;

fn edit<R>(
    cx: &mut TestAppContext,
    handle: Handle,
    f: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>) -> R,
) -> R {
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| f(shell, window, cx))
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

fn wire(shell: &mut Shell, side: usize, line: &str, window: &mut Window, cx: &mut Context<Shell>) {
    shell.receive(side, Backend::test_reply_line(line), window, cx);
}

fn commit(
    shell: &mut Shell,
    side: usize,
    path: &str,
    listing: usize,
    window: &mut Window,
    cx: &mut Context<Shell>,
) {
    let path = flea::json::escape(path);
    wire(
        shell,
        side,
        &format!(r#"{{"t":"listed","path":"{path}","n":2}}"#),
        window,
        cx,
    );
    wire(
        shell,
        side,
        &format!(
            r#"{{"t":"rows","start":0,"listing":{listing},"rows":[{{"n":"a","d":true,"s":0,"m":0,"p":16877,"i":"folder"}},{{"n":"b","d":true,"s":0,"m":0,"p":16877,"i":"folder"}}]}}"#
        ),
        window,
        cx,
    );
}

fn open(cx: &mut TestAppContext) -> (Handle, impl FnMut() -> Option<String>) {
    let (backend, requests) = Backend::native31_requests();
    let handle = cx.add_window(|window, cx| Root {
        shell: cx.new(|cx| {
            let mut shell = shell_with(backend, cx);
            shell.panes[0].as_mut().unwrap().pane = crate::pane::Pane::new("/files".into(), 620.);
            shell.panes[1] = Some(PaneHost::new("/right".into(), None, 620., &shell.locale));
            shell.lifecycle.attach(1);
            shell.view = View::Dual;
            commit(&mut shell, 0, "/files", 3, window, cx);
            commit(&mut shell, 1, "/right", 3, window, cx);
            window.focus(&shell.focus, cx);
            shell
        }),
    });
    cx.simulate_window_resize(handle.into(), gpui::size(gpui::px(2000.), gpui::px(700.)));
    frame(cx, handle);
    (handle, requests)
}

fn call(shell: &mut Shell, name: &str, window: &mut Window, cx: &mut Context<Shell>) -> Reply {
    let (answer, read) = std::sync::mpsc::sync_channel(1);
    shell.ipc(
        crate::ipc::server::Event::Call {
            request: Request::Call {
                target: "flea".into(),
                function: name.into(),
                arguments: vec![],
            },
            answer,
        },
        window,
        cx,
    );
    read.try_recv().unwrap().reply
}

fn answer(
    cx: &mut TestAppContext,
    handle: Handle,
    side: usize,
    path: &str,
    name: &str,
    class: &str,
) {
    edit(cx, handle, |shell, window, cx| {
        let line = format!(
            r#"{{"t":"fsinfo","path":"{}","fs":"{}","free":442000000000,"class":"{}","tagable":true}}"#,
            flea::json::escape(path),
            flea::json::escape(name),
            flea::json::escape(class)
        );
        wire(shell, side, &line, window, cx);
    });
    frame(cx, handle);
}

fn disk(cx: &mut TestAppContext, handle: Handle, expected: &str) {
    edit(cx, handle, |shell, window, cx| {
        let reply = call(shell, "statusFooterState", window, cx);
        assert_eq!(reply.code, 0);
        let state = jsondoc::parse(&reply.stdout).unwrap();
        assert_eq!(
            state.get("filesystem").and_then(Json::as_str),
            Some(expected),
            "real fsinfo did not reach the mounted footer"
        );
        assert!(shell.slots.status.is_some());
        let footer = shell.surfaces[0].footer.as_ref().unwrap().read(cx);
        let painted = footer
            .painted
            .get("disk")
            .expect("the actual disk producer painted");
        assert_eq!(painted.text.as_ref(), expected);
        assert!(f32::from(painted.bounds.size.width) > 0.);
        assert!(f32::from(painted.bounds.size.height) > 0.);
    });
}

fn advance(cx: &mut TestAppContext) {
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(120));
    cx.run_until_parked();
}

fn requests(read: &mut impl FnMut() -> Option<String>) -> Vec<Json> {
    std::iter::from_fn(read)
        .map(|line| jsondoc::parse(&line).unwrap())
        .collect()
}
