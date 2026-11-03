//! The six view readers run through the live window dispatcher and the List's actual frames.
use crate::{
    backend::{Backend, Event as BackendEvent},
    ipc::server::Event,
    list::pane::{Held, ROW_HEIGHT},
    state::{tests::shell_with, Shell},
    window::startup::View,
};
use flea::{
    ipc::wire::{Reply, Request},
    protocol::RowFacts,
};
use gpui::{px, size, AppContext, TestAppContext, TestDispatcher, WindowHandle};
use std::rc::Rc;

#[path = "view_geometry_tests.rs"]
mod geometry;

fn context() -> TestAppContext {
    TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        gpui_platform::current_platform(true).text_system(),
    )
}

fn held(listing: usize, start: usize, total: usize, name: &str) -> Rc<Held> {
    Rc::new(Held {
        listing,
        start,
        total,
        rows: (start..(start + 320).min(total))
            .map(|index| RowFacts {
                name: format!("{name}-{index}"),
                size: 26,
                icon: "text-plain".into(),
                mode: 0o100644,
                ..RowFacts::default()
            })
            .collect(),
    })
}

fn open(cx: &mut TestAppContext, backend: Backend) -> WindowHandle<Shell> {
    cx.open_window(size(px(900.), px(600.)), |_, cx| {
        let mut shell = shell_with(backend, cx);
        shell.panes[0].as_mut().unwrap().pane.held = held(2, 0, 100_000, "file");
        shell
    })
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
            call_now(shell, window, cx, function, arguments)
        })
        .unwrap()
}

fn call_now(
    shell: &mut Shell,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<Shell>,
    function: &str,
    arguments: &[&str],
) -> Reply {
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
    receive.try_recv().expect("the live reader answered").reply
}

fn value(cx: &mut TestAppContext, handle: WindowHandle<Shell>, function: &str) -> String {
    let reply = call(cx, handle, function, &[]);
    assert_eq!(reply.code, 0, "{function}: {}", reply.stderr);
    reply.stdout.trim_end_matches('\n').into()
}

#[test]
fn u02_live_readers_follow_the_drawn_window_without_fetching_rows() {
    let mut cx = context();
    let (backend, mut requests) = Backend::test_channel();
    let window = open(&mut cx, backend);
    draw(&mut cx, window);
    assert_eq!(value(&mut cx, window, "viewMode"), "list");
    assert_eq!(value(&mut cx, window, "drawnCount"), "100000");
    assert_eq!(
        call(&mut cx, window, "rowAt", &["0"]),
        Reply::value("file-0|file|26 B")
    );
    assert_eq!(
        call(&mut cx, window, "visibleRowName", &["0"]),
        Reply::value("file-0")
    );
    for index in ["-1", "31", "99999", "100000"] {
        assert_eq!(
            call(&mut cx, window, "rowAt", &[index]),
            Reply::value("loading")
        );
        assert_eq!(
            call(&mut cx, window, "visibleRowName", &[index]),
            Reply::value("")
        );
    }
    let (height, row, header) = window
        .update(&mut cx, |shell, _, _| {
            let list = shell.panes[0].as_ref().unwrap().list.borrow();
            (
                f32::from(list.drawn.bounds.size.height),
                list.drawn.metrics.row,
                list.drawn.metrics.header,
            )
        })
        .unwrap();
    assert_eq!(
        value(&mut cx, window, "visibleRows"),
        (height / row).ceil().max(1.).to_string()
    );
    assert_eq!(
        value(&mut cx, window, "viewEndY"),
        // reader-port-c: the end includes the header extent, as v0.3.12's contentHeight does.
        (100_000. * row + header - height).round().to_string()
    );
    assert!(
        requests().is_none(),
        "readers fetched rows from the backend"
    );

    window
        .update(&mut cx, |shell, _, _| {
            shell.panes[0]
                .as_mut()
                .unwrap()
                .pane
                .scroll_to(600. * ROW_HEIGHT);
        })
        .unwrap();
    draw(&mut cx, window);
    assert!(
        requests().is_some(),
        "the real frame asked for its missing window"
    );
    assert_eq!(
        call(&mut cx, window, "visibleRowName", &["600"]),
        Reply::value("")
    );
    assert_eq!(
        call(&mut cx, window, "rowAt", &["600"]),
        Reply::value("loading")
    );
    assert_eq!(
        call(&mut cx, window, "visibleRowName", &["0"]),
        Reply::value("")
    );
    assert_eq!(value(&mut cx, window, "drawnCount"), "100000");
    assert!(
        requests().is_none(),
        "a loading-row read fetched or scanned the directory"
    );
    for listing in [99, 2] {
        window
            .update(&mut cx, |shell, window, cx| {
                let request = shell.panes[0]
                    .as_ref()
                    .unwrap()
                    .pane
                    .requests
                    .outstanding
                    .unwrap();
                let mut rows = held(listing, request.start, 100_000, "file").rows.clone();
                rows.truncate(request.count);
                shell.receive(
                    0,
                    BackendEvent::Reply(flea::protocol::Reply::Rows {
                        start: request.start,
                        rows,
                        listing,
                    }),
                    window,
                    cx,
                );
            })
            .unwrap();
        draw(&mut cx, window);
        let expected = if listing == 2 { "file-600" } else { "" };
        assert_eq!(
            call(&mut cx, window, "visibleRowName", &["600"]),
            Reply::value(expected)
        );
    }
    assert_eq!(
        call(&mut cx, window, "rowAt", &["600"]),
        Reply::value("file-600|file|26 B")
    );
}

#[test]
fn u02_readers_hold_the_painted_snapshot_until_the_next_frame() {
    let mut cx = context();
    let (backend, _requests) = Backend::test_channel();
    let window = open(&mut cx, backend);
    draw(&mut cx, window);
    window
        .update(&mut cx, |shell, _, _| {
            shell.panes[0].as_mut().unwrap().pane.held = held(3, 0, 2, "new");
        })
        .unwrap();
    assert_eq!(
        call(&mut cx, window, "visibleRowName", &["0"]),
        Reply::value("file-0")
    );
    assert_eq!(
        call(&mut cx, window, "rowAt", &["0"]),
        Reply::value("file-0|file|26 B")
    );
    assert_eq!(value(&mut cx, window, "drawnCount"), "100000");
    draw(&mut cx, window);
    assert_eq!(
        call(&mut cx, window, "visibleRowName", &["0"]),
        Reply::value("new-0")
    );
    assert_eq!(value(&mut cx, window, "drawnCount"), "2");
    assert!(value(&mut cx, window, "viewEndY").parse::<i64>().unwrap() < 0);

    window
        .update(&mut cx, |shell, _, _| {
            let pane = &mut shell.panes[0].as_mut().unwrap().pane;
            pane.fail("backend", "", "", 0, pane.now());
        })
        .unwrap();
    assert_eq!(
        call(&mut cx, window, "visibleRowName", &["0"]),
        Reply::value("new-0")
    );
    assert_eq!(
        call(&mut cx, window, "rowAt", &["0"]),
        Reply::value("new-0|file|26 B")
    );
    assert_eq!(value(&mut cx, window, "drawnCount"), "2");
    draw(&mut cx, window);
    assert_eq!(
        call(&mut cx, window, "rowAt", &["0"]),
        Reply::value("loading")
    );
    assert_eq!(
        call(&mut cx, window, "visibleRowName", &["0"]),
        Reply::value("")
    );
    assert_eq!(value(&mut cx, window, "drawnCount"), "0");
    assert_eq!(call(&mut cx, window, "rowThumb", &["0"]), Reply::value(""));
}

#[test]
fn u02_readers_follow_dual_pane_focus_and_only_report_the_mounted_view() {
    let mut cx = context();
    let (backend, _requests) = Backend::test_channel();
    let window = open(&mut cx, backend);
    window
        .update(&mut cx, |shell, _, cx| {
            let mut right =
                crate::window::pane::PaneHost::new("/right".into(), None, 500., &shell.locale);
            right.pane.held = held(4, 0, 3, "right");
            right.pane.state.kind = crate::pane::states::Kind::Ready;
            shell.panes[1] = Some(right);
            shell.view = View::Dual;
            shell.focus_pane(1, cx);
        })
        .unwrap();
    draw(&mut cx, window);
    assert_eq!(
        call(&mut cx, window, "visibleRowName", &["0"]),
        Reply::value("right-0")
    );
    assert_eq!(value(&mut cx, window, "drawnCount"), "3");
    assert_eq!(value(&mut cx, window, "viewMode"), "list");
    window
        .update(&mut cx, |shell, _, cx| shell.focus_pane(0, cx))
        .unwrap();
    assert_eq!(
        call(&mut cx, window, "visibleRowName", &["0"]),
        Reply::value("file-0")
    );
    window
        .update(&mut cx, |shell, window, cx| {
            shell.set_view(View::Grid, window, cx);
            assert_eq!(shell.view, View::Grid);
            assert_eq!(
                call_now(shell, window, cx, "viewMode", &[]),
                Reply::value("list")
            );
        })
        .unwrap();
    draw(&mut cx, window);
    assert_eq!(value(&mut cx, window, "viewMode"), "grid");
}

#[test]
fn u02_view_mode_follows_the_drawn_frame_while_selection_changes() {
    let mut cx = context();
    let (backend, _requests) = Backend::test_channel();
    let window = cx.open_window(size(px(1100.), px(700.)), |_, cx| shell_with(backend, cx));
    draw(&mut cx, window);
    let mut previous_mode = "list";
    // Columns is selected but still entering: this fixture has not answered its peeks.
    for (view, expected_mode) in [
        (View::Columns, "list"),
        (View::List, "list"),
        (View::Grid, "grid"),
        (View::Dual, "list"),
        (View::List, "list"),
    ] {
        window
            .update(&mut cx, |shell, window, cx| {
                if view == View::Dual {
                    shell.view = View::Dual;
                } else {
                    shell.set_view(view, window, cx);
                }
                assert_eq!(
                    call_now(shell, window, cx, "viewMode", &[]),
                    Reply::value(previous_mode),
                    "selecting {view:?} must not rewrite the previous frame's mode"
                );
            })
            .unwrap();
        draw(&mut cx, window);
        let applied = window.update(&mut cx, |shell, _, _| shell.view).unwrap();
        assert_eq!(applied, view, "the scene applied another view");
        if view == View::Columns {
            assert!(
                window
                    .update(&mut cx, |shell, _, _| {
                        shell.panes[0]
                            .as_ref()
                            .unwrap()
                            .columns
                            .as_ref()
                            .unwrap()
                            .entering
                    })
                    .unwrap(),
                "unanswered Columns entry must retain the List frame"
            );
        }
        assert_eq!(value(&mut cx, window, "viewMode"), expected_mode);
        previous_mode = expected_mode;
    }
    let second = window
        .update(&mut cx, |shell, _, _| {
            shell.panes[1].as_ref().map(|host| host.drawn_view)
        })
        .unwrap();
    assert_ne!(
        second,
        Some(Some(View::Dual)),
        "the hidden pane kept its dual record"
    );
}

// EY-441, Ipc.qml drawnCount is listArea.count: a standing filter answers the rows it narrowed to.
#[test]
fn drawn_count_answers_the_filter_rows_the_list_drew() {
    let mut cx = context();
    let (backend, _requests) = Backend::test_channel();
    let window = open(&mut cx, backend);
    draw(&mut cx, window);
    assert_eq!(value(&mut cx, window, "drawnCount"), "100000");
    window
        .update(&mut cx, |shell, _, _| {
            shell.panes[0].as_mut().unwrap().filter.shown = Some(Rc::from([1usize, 3]));
        })
        .unwrap();
    draw(&mut cx, window);
    assert_eq!(value(&mut cx, window, "drawnCount"), "2");
    window
        .update(&mut cx, |shell, _, _| {
            shell.panes[0].as_mut().unwrap().filter.shown = None;
        })
        .unwrap();
    draw(&mut cx, window);
    assert_eq!(value(&mut cx, window, "drawnCount"), "100000");
}
