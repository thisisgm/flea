//! Every row reader follows the real window from an unpainted listing to its mounted frame.
use crate::{
    backend::{Backend, Event as BackendEvent},
    ipc::server::Event,
    pane::{states::Kind, Pane},
    state::{tests::shell_with, Shell},
    window::startup::View,
};
use flea::{
    ipc::wire::{Reply, Request},
    protocol::{Reply as BackendReply, RowFacts},
};
use gpui::{px, size, AppContext, TestAppContext, TestDispatcher, WindowHandle};

fn context() -> TestAppContext {
    TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        gpui_platform::current_platform(true).text_system(),
    )
}

fn open(
    cx: &mut TestAppContext,
    view: View,
) -> (WindowHandle<Shell>, impl FnMut() -> Option<(usize, usize)>) {
    let (backend, requests) = Backend::test_channel();
    let window = cx.open_window(size(px(1100.), px(700.)), |_, cx| {
        let mut shell = shell_with(backend, cx);
        let host = shell.panes[0].as_mut().unwrap();
        host.pane = Pane::new("/".into(), 646.);
        host.path = "/".into();
        host.pane.state.kind = Kind::Loading;
        shell.view = view;
        shell
    });
    (window, requests)
}

fn draw(cx: &mut TestAppContext, handle: WindowHandle<Shell>) {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
}

fn call(
    cx: &mut TestAppContext,
    handle: WindowHandle<Shell>,
    function: &str,
    arguments: &[&str],
) -> Reply {
    handle
        .update(cx, |shell, window, cx| {
            let (answer, receive) = std::sync::mpsc::sync_channel(1);
            shell.ipc(
                Event::Call {
                    request: Request::Call {
                        target: "flea".into(),
                        function: function.into(),
                        arguments: arguments.iter().map(|value| (*value).into()).collect(),
                    },
                    answer,
                },
                window,
                cx,
            );
            receive.try_recv().unwrap().reply
        })
        .unwrap()
}

fn ready(cx: &mut TestAppContext, handle: WindowHandle<Shell>, name: &str) {
    handle
        .update(cx, |shell, window, cx| {
            let first = shell.panes[0].as_ref().unwrap().pane.first_count();
            for reply in [
                BackendReply::Listed {
                    path: "/".into(),
                    total: 1000,
                },
                BackendReply::Rows {
                    start: 0,
                    listing: 7,
                    rows: (0..first)
                        .map(|index| RowFacts {
                            name: format!("{name}-{index}"),
                            dir: index == 1,
                            size: 18_000,
                            icon: "text-plain".into(),
                            mode: 0o100644,
                            ..RowFacts::default()
                        })
                        .collect(),
                },
            ] {
                shell.receive(0, BackendEvent::Reply(reply), window, cx);
            }
        })
        .unwrap();
}

fn transition(view: View, function: &str, pending: &str, expected: &str) {
    let mut cx = context();
    let (window, _requests) = open(&mut cx, view);
    let arguments: &[&str] = if function == "drawnCount" {
        &[]
    } else {
        &["0"]
    };
    assert_eq!(
        call(&mut cx, window, function, arguments),
        Reply::value(pending)
    );
    draw(&mut cx, window);
    assert_eq!(
        call(&mut cx, window, function, arguments),
        Reply::value(pending)
    );
    ready(&mut cx, window, "current");
    draw(&mut cx, window);
    assert_eq!(
        call(&mut cx, window, "viewMode", &[]),
        Reply::value(if view == View::Grid {
            "grid"
        } else {
            "columns"
        })
    );
    assert_eq!(call(&mut cx, window, "path", &[]), Reply::value("/"));
    assert_eq!(
        call(&mut cx, window, function, arguments),
        Reply::value(expected)
    );
}

#[test]
fn columns_row_at_loading_then_ready() {
    transition(View::Columns, "rowAt", "loading", "current-0|file|18.0 kB");
}

#[test]
fn grid_row_at_loading_then_ready() {
    transition(View::Grid, "rowAt", "loading", "current-0|file|18.0 kB");
}

#[test]
fn columns_visible_row_name_loading_then_ready() {
    transition(View::Columns, "visibleRowName", "", "current-0");
}

#[test]
fn grid_visible_row_name_loading_then_ready() {
    transition(View::Grid, "visibleRowName", "", "current-0");
}

#[test]
fn columns_drawn_count_loading_then_ready() {
    transition(View::Columns, "drawnCount", "0", "1000");
}

#[test]
fn grid_drawn_count_loading_then_ready() {
    transition(View::Grid, "drawnCount", "0", "1000");
}

#[test]
fn row_readers_keep_the_columns_and_grid_painted_snapshot() {
    for view in [View::Columns, View::Grid] {
        let mut cx = context();
        let (window, _requests) = open(&mut cx, view);
        ready(&mut cx, window, "current");
        draw(&mut cx, window);
        window
            .update(&mut cx, |shell, _, _| {
                let pane = &mut shell.panes[0].as_mut().unwrap().pane;
                pane.held = std::rc::Rc::new(crate::list::pane::Held {
                    listing: 8,
                    start: 0,
                    total: 2,
                    rows: vec![
                        RowFacts {
                            name: "replacement".into(),
                            size: 26,
                            ..RowFacts::default()
                        };
                        2
                    ],
                });
            })
            .unwrap();
        for (function, arguments, expected) in [
            ("rowAt", vec!["0"], "current-0|file|18.0 kB"),
            ("visibleRowName", vec!["0"], "current-0"),
            ("drawnCount", vec![], "1000"),
        ] {
            assert_eq!(
                call(&mut cx, window, function, &arguments),
                Reply::value(expected)
            );
        }
        draw(&mut cx, window);
        for (function, arguments, expected) in [
            ("rowAt", vec!["0"], "replacement|file|26 B"),
            ("visibleRowName", vec!["0"], "replacement"),
            ("drawnCount", vec![], "2"),
        ] {
            assert_eq!(
                call(&mut cx, window, function, &arguments),
                Reply::value(expected)
            );
        }
    }
}

#[test]
fn columns_and_grid_row_facts_keep_kind_and_viewport_bounds() {
    for view in [View::Columns, View::Grid] {
        let mut cx = context();
        let (window, mut requests) = open(&mut cx, view);
        ready(&mut cx, window, "current");
        draw(&mut cx, window);
        assert_eq!(
            call(&mut cx, window, "rowAt", &["1"]),
            Reply::value("current-1|dir|18.0 kB")
        );
        while requests().is_some() {}
        for index in ["-1", "999", "1000"] {
            assert_eq!(
                call(&mut cx, window, "rowAt", &[index]),
                Reply::value("loading")
            );
            assert_eq!(
                call(&mut cx, window, "visibleRowName", &[index]),
                Reply::value("")
            );
        }
        assert!(requests().is_none(), "readers must not fetch rows");
        assert_eq!(
            call(&mut cx, window, "drawnCount", &[]),
            Reply::value("1000")
        );
    }
}
