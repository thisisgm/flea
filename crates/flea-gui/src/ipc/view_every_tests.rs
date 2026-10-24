//! Every row reader follows the real window from an unpainted listing to its mounted frame.
use crate::{
    backend::{Backend, Event as BackendEvent},
    columns::geometry::Slot,
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

fn assert_rows(cx: &mut TestAppContext, handle: WindowHandle<Shell>, expected: [&str; 3]) {
    for ((function, arguments), expected) in [
        ("rowAt", &["0"][..]),
        ("visibleRowName", &["0"][..]),
        ("drawnCount", &[][..]),
    ]
    .into_iter()
    .zip(expected)
    {
        assert_eq!(
            call(cx, handle, function, arguments),
            Reply::value(expected),
            "{function}"
        );
    }
}

fn ready(cx: &mut TestAppContext, handle: WindowHandle<Shell>, name: &str, total: usize) {
    handle
        .update(cx, |shell, window, cx| {
            let first = shell.panes[0].as_ref().unwrap().pane.first_count();
            for reply in [
                BackendReply::Listed {
                    path: "/".into(),
                    total,
                },
                BackendReply::Rows {
                    start: 0,
                    listing: 7,
                    rows: (0..first.min(total))
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
    ready(&mut cx, window, "current", 1000);
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
        ready(&mut cx, window, "current", 1000);
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
        assert_rows(
            &mut cx,
            window,
            ["current-0|file|18.0 kB", "current-0", "1000"],
        );
        draw(&mut cx, window);
        assert_rows(
            &mut cx,
            window,
            ["replacement|file|26 B", "replacement", "2"],
        );
    }
}

#[test]
fn columns_and_grid_row_facts_keep_kind_and_viewport_bounds() {
    for view in [View::Columns, View::Grid] {
        let mut cx = context();
        let (window, mut requests) = open(&mut cx, view);
        ready(&mut cx, window, "current", 1000);
        draw(&mut cx, window);
        assert_eq!(
            call(&mut cx, window, "rowAt", &["1"]),
            Reply::value("current-1|dir|18.0 kB")
        );
        while requests().is_some() {}
        let held_undrawn = window
            .update(&mut cx, |shell, _, _| {
                let host = shell.panes[0].as_ref().unwrap();
                let held = &host.pane.held;
                (held.start..held.start + held.rows.len())
                    .find(|&index| match view {
                        View::Columns => host
                            .columns
                            .as_ref()
                            .unwrap()
                            .painters
                            .get(Slot::Active)
                            .unwrap()
                            .borrow()
                            .prepared(index)
                            .is_none(),
                        View::Grid => host
                            .grid
                            .as_ref()
                            .unwrap()
                            .state
                            .borrow()
                            .drawn_row(index)
                            .is_none(),
                        _ => unreachable!(),
                    })
                    .expect("the bounded held window includes rows beyond the viewport")
                    .to_string()
            })
            .unwrap();
        println!("HELD_UNDRAWN {view:?} index={held_undrawn}");
        for index in ["-1", "999", "1000", &held_undrawn] {
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

fn surface_after_painted_rows(view: View, kind: Kind) {
    let mut cx = context();
    let (window, _requests) = open(&mut cx, view);
    ready(&mut cx, window, "current", 1);
    draw(&mut cx, window);
    assert_eq!(
        call(&mut cx, window, "rowAt", &["0"]),
        Reply::value("current-0|file|18.0 kB")
    );
    window
        .update(&mut cx, |shell, window, cx| {
            let path = if kind == Kind::Empty { "/" } else { "/missing" };
            let pane = &mut shell.panes[0].as_mut().unwrap().pane;
            let now = pane.now();
            assert!(pane.begin(path.into(), crate::pane::swap::Options::default(), now));
            match kind {
                Kind::Loading => {
                    assert!(pane.tick(now + crate::pane::swap::CAP_MS));
                    assert!(pane.swap.fell_back, "the real cap expired");
                }
                Kind::Empty => {
                    for reply in [
                        BackendReply::Listed {
                            path: path.into(),
                            total: 0,
                        },
                        BackendReply::Rows {
                            start: 0,
                            listing: 8,
                            rows: vec![],
                        },
                    ] {
                        shell.receive(0, BackendEvent::Reply(reply), window, cx);
                    }
                }
                Kind::Error => shell.receive(
                    0,
                    BackendEvent::Reply(BackendReply::Error {
                        operation: "scan".into(),
                        path: path.into(),
                        message: "I/O error".into(),
                        mode: 0,
                    }),
                    window,
                    cx,
                ),
                _ => unreachable!(),
            }
            assert_eq!(shell.panes[0].as_ref().unwrap().pane.state.kind, kind);
        })
        .unwrap();
    draw(&mut cx, window);
    assert_rows(&mut cx, window, ["loading", "", "0"]);
    window
        .update(&mut cx, |shell, _, _| {
            assert_eq!(shell.mark_drawn(0), kind == Kind::Loading);
            assert_eq!(shell.slots.states[0].is_some(), kind != Kind::Loading);
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.drawn_view, Some(view));
            assert_eq!(
                host.pane.held.total, 0,
                "the transition cleared the old listing"
            );
            match view {
                View::Columns => {
                    let columns = host.columns.as_ref().unwrap();
                    let paint = columns.painters.get(Slot::Active).unwrap().borrow();
                    let drawn = paint.drawn.as_ref().unwrap();
                    let held = drawn.held.as_ref().unwrap();
                    assert_eq!(held.total, 0);
                    assert!(paint.prepared(0).is_none());
                }
                View::Grid => {
                    let state = host.grid.as_ref().unwrap().state.borrow();
                    assert!(state.frame_held.is_none());
                    assert!(state.drawn_row(0).is_none());
                }
                _ => unreachable!(),
            }
        })
        .unwrap();
    println!("SURFACE {view:?} {kind:?}: rowAt=loading visibleRowName=empty drawnCount=0; old facts cleared");
}

#[test]
fn columns_rows_disappear_after_loading_cap() {
    surface_after_painted_rows(View::Columns, Kind::Loading);
}

#[test]
fn grid_rows_disappear_after_loading_cap() {
    surface_after_painted_rows(View::Grid, Kind::Loading);
}

#[test]
fn columns_rows_disappear_after_last_file_deleted() {
    surface_after_painted_rows(View::Columns, Kind::Empty);
}

#[test]
fn grid_rows_disappear_after_last_file_deleted() {
    surface_after_painted_rows(View::Grid, Kind::Empty);
}

#[test]
fn columns_rows_disappear_after_failed_navigation() {
    surface_after_painted_rows(View::Columns, Kind::Error);
}

#[test]
fn grid_rows_disappear_after_failed_navigation() {
    surface_after_painted_rows(View::Grid, Kind::Error);
}
