//! Grid landing windows settle only on their own rows or real backend refusal.
use super::join_tests::{call, frame, key, named_shell};
use crate::{
    backend::{Backend, Event},
    window::startup::View,
};
use flea::{ipc::wire::Reply, protocol};
use gpui::TestAppContext;

#[gpui::test]
fn u10_grid_requests_only_its_landing_window_and_a_refusal_does_not_loop(cx: &mut TestAppContext) {
    let (backend, mut requests) = Backend::test_channel();
    let handle = cx.add_window(|_, cx| named_shell(backend, cx));
    handle
        .update(cx, |shell, window, cx| {
            shell.set_view(View::Grid, window, cx)
        })
        .unwrap();
    frame(cx, handle);
    while requests().is_some() {}
    key(cx, handle, "end");
    let request = handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            let grid = host.grid.as_ref().unwrap();
            let state = grid.state.borrow();
            assert!(
                state.drawn().len() < 100,
                "integer-count Grid keeps its renderer viewport sized"
            );
            assert!(
                grid.view.landing_wait,
                "the old window stays drawn until the cursor rows land"
            );
            host.pane.requests.outstanding.unwrap()
        })
        .unwrap();
    assert!(request.start > 600);
    assert!(request.count < 200);
    assert!(requests().is_some());
    for _ in 0..3 {
        frame(cx, handle);
    }
    assert_eq!(
        requests(),
        None,
        "an outstanding window is never asked twice"
    );
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
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert!(!host.grid.as_ref().unwrap().view.landing_wait);
            assert!(host.pane.requests.outstanding.is_none());
        })
        .unwrap();
    assert_eq!(
        requests(),
        None,
        "the refused landing is not retried every frame"
    );
    handle
        .update(cx, |shell, window, cx| {
            shell.set_view(View::List, window, cx)
        })
        .unwrap();
    frame(cx, handle);
    handle
        .update(cx, |shell, _, _| {
            let grid = shell.panes[0].as_ref().unwrap().grid.as_ref().unwrap();
            assert!(!grid.view.visible);
            assert_eq!(grid.state.borrow().cached_tiles(), 0);
            assert!(grid.state.borrow().drawn().is_empty());
        })
        .unwrap();
}

#[gpui::test]
fn u10_grid_far_landing_waits_for_its_actual_rows_then_reveals_the_cursor(cx: &mut TestAppContext) {
    let (backend, mut requests) = Backend::test_channel();
    let handle = cx.add_window(|_, cx| named_shell(backend, cx));
    handle
        .update(cx, |shell, window, cx| {
            shell.set_view(View::Grid, window, cx)
        })
        .unwrap();
    frame(cx, handle);
    while requests().is_some() {}
    key(cx, handle, "end");
    assert!(requests().is_some());
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            let state = host.grid.as_ref().unwrap().state.borrow();
            assert_eq!(host.pane.cursor, 999, "the request targets the new cursor");
            assert!(
                state.drawn_row(0).unwrap().cursor,
                "the complete old frame keeps its cursor until the new rows land"
            );
            assert!(state
                .drawn()
                .iter()
                .all(|tile| tile.caption_text().is_some_and(|text| !text.is_empty())));
        })
        .unwrap();
    handle
        .update(cx, |shell, window, cx| {
            let host = shell.panes[0].as_ref().unwrap();
            let ask = host.pane.requests.outstanding.unwrap();
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
    let diagnostic = handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            let grid = host.grid.as_ref().unwrap();
            let state = grid.state.borrow();
            format!(
                "cursor={} held={}+{} busy={} kind={:?} view={:?} layout={:?} frame_scroll={} bounds={:?} drawn={:?}",
                host.pane.cursor,
                host.pane.held.start,
                host.pane.held.rows.len(),
                host.pane.busy(),
                host.pane.state.kind,
                grid.view,
                state.layout,
                state.scroll,
                state.bounds,
                state.drawn().iter().map(|d| d.listing).collect::<Vec<_>>()
            )
        })
        .unwrap();
    assert_eq!(
        call(cx, handle, "visibleRowName", &["999"]),
        Reply::value("file-999.txt"),
        "{diagnostic}"
    );
    assert!(!call(cx, handle, "rowCentre", &["999"])
        .stdout
        .trim()
        .is_empty());
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            let grid = host.grid.as_ref().unwrap();
            assert!(!grid.view.landing_wait);
            assert!(grid.state.borrow().drawn_row(999).unwrap().cursor);
            assert!(grid.view.scroll > 0.);
            let state = grid.state.borrow();
            let layout = state.layout.unwrap();
            assert!(layout.covers(state.scroll, 999));
            assert!(state.drawn().len() < 100);
        })
        .unwrap();
}
