//! Review proofs for uish-u10 round 1; each asserts the behaviour the brief asks for.
use super::join_tests::{call, frame, key, named_shell};
use crate::{
    backend::{Backend, Event},
    state::Shell,
    window::startup::View,
};
use flea::protocol;
use gpui::{TestAppContext, VisualTestContext, WindowHandle};

fn rows_for_outstanding(cx: &mut TestAppContext, handle: WindowHandle<Shell>) {
    handle
        .update(cx, |shell, window, cx| {
            let host = shell.panes[0].as_ref().unwrap();
            let ask = host
                .pane
                .requests
                .outstanding
                .expect("an outstanding window");
            let end = (ask.start + ask.count).min(host.pane.held.total);
            shell.receive(
                0,
                Event::Reply(protocol::Reply::Rows {
                    start: ask.start,
                    listing: ask.listing,
                    rows: (ask.start..end)
                        .map(|index| protocol::RowFacts {
                            name: format!("file-{index}.txt"),
                            ..Default::default()
                        })
                        .collect(),
                }),
                window,
                cx,
            );
        })
        .unwrap();
    frame(cx, handle);
}

fn refuse(cx: &mut TestAppContext, handle: WindowHandle<Shell>) {
    handle
        .update(cx, |shell, window, cx| {
            shell.receive(
                0,
                Event::Refused {
                    operation: "window".into(),
                    path: "/files".into(),
                    message: "Permission denied".into(),
                    mode: 0,
                },
                window,
                cx,
            )
        })
        .unwrap();
    for _ in 0..3 {
        frame(cx, handle);
    }
}

// Wheel the Grid to the end so the held window moves off row 0, with the cursor left on row 0.
fn grid_scrolled_to_end(
    cx: &mut TestAppContext,
) -> (WindowHandle<Shell>, impl FnMut() -> Option<(usize, usize)>) {
    let (backend, requests) = Backend::test_channel();
    let handle = cx.add_window(|_, cx| named_shell(backend, cx));
    handle
        .update(cx, |shell, window, cx| {
            shell.set_view(View::Grid, window, cx)
        })
        .unwrap();
    frame(cx, handle);
    handle
        .update(
            cx,
            |shell, _, cx| assert!(shell.grid_scroll(1_000_000., cx)),
        )
        .unwrap();
    frame(cx, handle);
    rows_for_outstanding(cx, handle);
    frame(cx, handle);
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.pane.cursor, 0);
            assert!(host.pane.held.start > 0, "the drift window replaced row 0");
            assert!(host.pane.held.row(0).is_none());
            let state = host.grid.as_ref().unwrap().state.borrow();
            assert!(state.drawn().iter().all(|tile| !tile.cursor));
        })
        .unwrap();
    (handle, requests)
}

#[gpui::test]
fn rev_u10_r1_a_refused_landing_leaves_the_cursor_on_a_row_the_operator_chose(
    cx: &mut TestAppContext,
) {
    let (handle, _requests) = grid_scrolled_to_end(cx);
    let (columns, held_start) = handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            let state = host.grid.as_ref().unwrap().state.borrow();
            (state.layout.unwrap().columns, host.pane.held.start)
        })
        .unwrap();
    key(cx, handle, "down");
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.pane.cursor, columns);
            assert!(host.grid.as_ref().unwrap().view.landing_wait);
        })
        .unwrap();
    refuse(cx, handle);
    let cursor = handle
        .update(cx, |shell, _, _| {
            shell.panes[0].as_ref().unwrap().pane.cursor
        })
        .unwrap();
    assert!(
        cursor == 0 || cursor == columns,
        "a refused landing moved the cursor to row {cursor} (held.start {held_start}), neither the old cursor 0 nor the asked {columns}"
    );
}

#[gpui::test]
fn rev_u10_r1_a_refused_list_fetch_still_lands_the_requested_view(cx: &mut TestAppContext) {
    let (handle, _requests) = grid_scrolled_to_end(cx);
    handle
        .update(cx, |shell, window, cx| {
            shell.set_view(View::List, window, cx);
            assert_eq!(
                shell.view,
                View::Grid,
                "the switch is staged behind a fetch"
            );
        })
        .unwrap();
    frame(cx, handle);
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert!(host.pane.requests.outstanding.is_some());
        })
        .unwrap();
    refuse(cx, handle);
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            let staged = host
                .grid
                .as_ref()
                .is_some_and(|grid| grid.next_view.is_some());
            assert!(
                shell.view == View::List || staged,
                "the refused fetch dropped the operator's List switch: view {:?}, staged {staged}, cursor {} held {}+{}",
                shell.view,
                host.pane.cursor,
                host.pane.held.start,
                host.pane.held.rows.len()
            );
        })
        .unwrap();
}

#[gpui::test]
fn rev_u10_r1_a_reload_during_a_staged_switch_still_lands_the_requested_view(
    cx: &mut TestAppContext,
) {
    let (backend, _requests) = Backend::test_channel();
    let handle = cx.add_window(|_, cx| named_shell(backend, cx));
    handle
        .update(cx, |shell, window, cx| {
            shell.set_view(View::Grid, window, cx)
        })
        .unwrap();
    frame(cx, handle);
    handle
        .update(cx, |shell, window, cx| {
            shell.panes[0].as_mut().unwrap().pane.cursor = 999;
            shell.set_view(View::List, window, cx);
            assert_eq!(
                shell.view,
                View::Grid,
                "the switch is staged behind a fetch"
            );
        })
        .unwrap();
    frame(cx, handle);
    handle
        .update(cx, |shell, _, _| {
            assert!(shell.panes[0].as_mut().unwrap().pane.begin(
                "/files".into(),
                crate::pane::swap::Options::default(),
                0,
            ));
        })
        .unwrap();
    for _ in 0..3 {
        frame(cx, handle);
    }
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            let staged = host
                .grid
                .as_ref()
                .is_some_and(|grid| grid.next_view.is_some());
            assert!(
                shell.view == View::List || staged,
                "a reload forgot the operator's List switch: view {:?}, staged {staged}, busy {}",
                shell.view,
                host.pane.busy()
            );
        })
        .unwrap();
}

#[gpui::test]
fn rev_u10_r1_a_failed_grid_window_send_ends_on_the_error_surface(cx: &mut TestAppContext) {
    let (backend, requests) = Backend::test_channel();
    let handle = cx.add_window(|_, cx| named_shell(backend, cx));
    drop(requests);
    handle
        .update(cx, |shell, window, cx| {
            shell.set_view(View::Grid, window, cx)
        })
        .unwrap();
    frame(cx, handle);
    key(cx, handle, "end");
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            let painted = host
                .grid
                .as_ref()
                .unwrap()
                .state
                .borrow()
                .drawn()
                .iter()
                .find(|tile| tile.cursor)
                .map(|tile| tile.listing);
            assert!(
                host.pane.state.kind == crate::pane::states::Kind::Error,
                "send failed (stopped {}, error {:?}) but the pane is {:?} with cursor {} painted at {painted:?}, landing_wait {}",
                host.stopped,
                host.error,
                host.pane.state.kind,
                host.pane.cursor,
                host.grid.as_ref().unwrap().view.landing_wait
            );
        })
        .unwrap();
}

#[gpui::test]
fn rev_u10_r1_grid_visible_rows_keeps_the_overlays_row_inside_the_area(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let handle = cx.add_window(|_, cx| named_shell(backend, cx));
    handle
        .update(cx, |shell, window, cx| {
            shell.set_view(View::Grid, window, cx)
        })
        .unwrap();
    frame(cx, handle);
    let mut misses = Vec::new();
    let mut seen = Vec::new();
    for height in (480..=1080).step_by(15) {
        {
            let visual = VisualTestContext::from_window(handle.into(), cx);
            visual.simulate_resize(gpui::size(gpui::px(1200.), gpui::px(height as f32)));
        }
        frame(cx, handle);
        key(cx, handle, "home");
        let visible: usize = call(cx, handle, "visibleRows", &[])
            .stdout
            .trim()
            .parse()
            .unwrap();
        let n = visible - 3;
        let centre = call(cx, handle, "rowCentre", &[&n.to_string()]).stdout;
        let area = call(cx, handle, "listAreaRect", &[]).stdout;
        let cy: i64 = centre.split_whitespace().nth(1).unwrap().parse().unwrap();
        let a: Vec<i64> = area
            .split_whitespace()
            .map(|v| v.parse().unwrap())
            .collect();
        seen.push(format!(
            "h{height}:rows{visible}:n{n}:cy{cy}:area{}+{}",
            a[1], a[3]
        ));
        if cy >= a[1] + a[3] {
            misses.push(format!(
                "height {height}: visibleRows {visible}, row {n} centre y {cy}, area bottom {}",
                a[1] + a[3]
            ));
        }
    }
    assert!(
        misses.is_empty(),
        "case_overlays hovers row visibleRows-3; in Grid that row's centre is below the listing: {misses:?}; all {seen:?}"
    );
}
