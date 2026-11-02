//! The prerequisite composition is exercised through the mounted shell and its producer deliveries.
use crate::{
    backend::{Backend, Event as BackendEvent},
    ipc::server::Event,
    state::{tests::shell_with, Shell},
    window::{pane::PaneHost, root::Root, startup::View},
};
use flea::{
    ipc::wire::{Reply, Request},
    jsondoc::Json,
};
use gpui::{AppContext, Context, TestAppContext, Window, WindowHandle};

#[path = "gcomp_background_menu_tests.rs"]
mod background_menu;
#[path = "gcomp_batch_tests.rs"]
mod batch;
#[path = "footer_feedback_tests.rs"]
mod footer_feedback;
#[path = "gcomp_geometry_tests.rs"]
mod geometry;
#[path = "gcomp_input_tests.rs"]
mod input;
#[path = "keynav_tests.rs"]
mod keynav;
#[path = "gcomp_live_tests.rs"]
mod live;
#[path = "gcomp_menu_tests.rs"]
mod menu;
#[path = "gcomp_operation_tests.rs"]
pub(super) mod operations;
#[path = "gcomp_path_tests.rs"]
mod path;
#[path = "gcomp_pathtab_tests.rs"]
mod pathtab;
#[path = "gcomp_presentation_tests.rs"]
mod presentation;
#[path = "gcomp_status_tests.rs"]
mod status;

type Handle = WindowHandle<Root>;

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
                    total: 8,
                },
                flea::protocol::Reply::Rows {
                    start: 0,
                    listing: 3,
                    rows: (0..8)
                        .map(|at| flea::protocol::RowFacts {
                            name: format!("file-{at}.txt"),
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

fn call(cx: &mut TestAppContext, handle: Handle, function: &str) -> Reply {
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
        receive.try_recv().unwrap().reply
    })
}

fn json(cx: &mut TestAppContext, handle: Handle, function: &str) -> Json {
    let reply = call(cx, handle, function);
    assert_eq!(reply.code, 0, "{function}: {}", reply.stderr);
    flea::jsondoc::parse(reply.stdout.trim()).unwrap()
}
