//! A deferred Grid switch preserves a full frame through bounded fetching and failure.
use super::join_tests::{frame, key, named_shell};
use crate::{
    backend::{Backend, Event},
    window::startup::View,
};
use flea::protocol;
use gpui::TestAppContext;

#[gpui::test]
fn u10_grid_view_switch_waits_for_complete_rows_and_keeps_a_complete_failure_frame(
    cx: &mut TestAppContext,
) {
    for outcome in [
        "rows",
        "refused",
        "failed",
        "destination-refused",
        "short",
        "oversized",
    ] {
        let (backend, mut requests) = Backend::test_channel();
        let handle = cx.add_window(|_, cx| named_shell(backend, cx));
        handle
            .update(cx, |shell, window, cx| {
                shell.set_view(View::Grid, window, cx)
            })
            .unwrap();
        {
            let visual = gpui::VisualTestContext::from_window(handle.into(), cx);
            visual.simulate_resize(gpui::size(gpui::px(500.), gpui::px(1440.)));
        }
        frame(cx, handle);
        handle
            .update(cx, |shell, _, _| {
                shell.panes[0]
                    .as_mut()
                    .unwrap()
                    .grid
                    .as_mut()
                    .unwrap()
                    .view
                    .thumb = super::geometry::ThumbSize::named("largest");
            })
            .unwrap();
        frame(cx, handle);
        while requests().is_some() {}
        key(cx, handle, "end");
        assert!(requests().is_some());
        handle
            .update(cx, |shell, window, cx| {
                shell.set_view(View::List, window, cx);
                let old_scroll = shell.panes[0]
                    .as_ref()
                    .unwrap()
                    .grid
                    .as_ref()
                    .unwrap()
                    .view
                    .scroll;
                assert!(shell.grid_scroll(100_000., cx));
                assert_eq!(
                    shell.panes[0]
                        .as_ref()
                        .unwrap()
                        .grid
                        .as_ref()
                        .unwrap()
                        .view
                        .scroll,
                    old_scroll,
                    "a deferred switch keeps its saved frame inside the saved held rows"
                );
                assert_eq!(
                    shell.view,
                    View::Grid,
                    "the old complete surface stays until the target can be drawn"
                );
            })
            .unwrap();
        frame(cx, handle);
        handle
            .update(cx, |shell, window, cx| {
                let host = shell.panes[0].as_ref().unwrap();
                let ask = host.pane.requests.outstanding.unwrap();
                let grid = host.grid.as_ref().unwrap().state.borrow();
                assert!(grid.drawn_row(0).unwrap().cursor);
                assert!(grid
                    .drawn()
                    .iter()
                    .all(|tile| tile.caption_text().is_some_and(|text| !text.is_empty())));
                drop(grid);
                let event = match outcome {
                    "rows" | "destination-refused" | "short" | "oversized" => {
                        Event::Reply(protocol::Reply::Rows {
                            start: ask.start,
                            listing: ask.listing,
                            rows: (ask.start..(ask.start + ask.count).min(host.pane.held.total))
                                .map(|index| protocol::RowFacts {
                                    name: format!("file-{index}.txt"),
                                    ..Default::default()
                                })
                                .collect(),
                        })
                    }
                    "refused" => Event::Refused {
                        operation: "window".into(),
                        path: "/files".into(),
                        message: "Permission denied".into(),
                        mode: 0,
                    },
                    _ => Event::Failed(crate::backend::Failure {
                        key: "window.backend_stopped",
                        detail: String::new(),
                    }),
                };
                shell.receive(0, event, window, cx);
            })
            .unwrap();
        frame(cx, handle);
        if matches!(
            outcome,
            "rows" | "destination-refused" | "short" | "oversized"
        ) {
            handle
                .update(cx, |shell, window, cx| {
                    assert_eq!(
                        shell.view,
                        View::Grid,
                        "one Grid landing cannot fill the taller List viewport"
                    );
                    let host = shell.panes[0].as_ref().unwrap();
                    let state = host.grid.as_ref().unwrap().state.borrow();
                    assert!(state.drawn_row(0).unwrap().cursor);
                    assert!(state.drawn().iter().all(|tile| tile.name().is_some()));
                    drop(state);
                    let ask = host.pane.requests.outstanding.unwrap();
                    assert!(
                        ask.count < 400,
                        "the destination fetch stays viewport bounded"
                    );
                    let event = if outcome == "destination-refused" {
                        Event::Refused {
                            operation: "window".into(),
                            path: "/files".into(),
                            message: "Permission denied".into(),
                            mode: 0,
                        }
                    } else {
                        let end = (ask.start + ask.count).min(host.pane.held.total);
                        let end = match outcome {
                            "short" => end - 1,
                            "oversized" => end + 1,
                            _ => end,
                        };
                        Event::Reply(protocol::Reply::Rows {
                            start: ask.start,
                            listing: ask.listing,
                            rows: (ask.start..end)
                                .map(|index| protocol::RowFacts {
                                    name: format!("file-{index}.txt"),
                                    ..Default::default()
                                })
                                .collect(),
                        })
                    };
                    shell.receive(0, event, window, cx);
                })
                .unwrap();
            frame(cx, handle);
        }
        handle
            .update(cx, |shell, _, _| {
                assert_eq!(
                    shell.view,
                    View::List,
                    "the requested view lands with its complete result"
                );
                let host = shell.panes[0].as_ref().unwrap();
                if outcome == "failed" {
                    assert_eq!(host.pane.state.kind, crate::pane::states::Kind::Error);
                    assert!(
                        shell.slots.states[0].is_some(),
                        "the completed failure surface replaces the rows"
                    );
                } else {
                    let expected = if outcome == "rows" { 999 } else { 0 };
                    assert_eq!(host.pane.cursor, expected);
                    let list = host.list.borrow();
                    assert!(list
                        .visible()
                        .iter()
                        .any(|row| row.index == expected && row.prepared.is_some()));
                    assert!(
                        list.visible().iter().all(|row| row.prepared.is_some()),
                        "the first List frame has no empty rows"
                    );
                }
            })
            .unwrap();
    }
}

#[gpui::test]
fn u10_grid_pending_switch_finishes_on_disconnected_or_missing_backend(cx: &mut TestAppContext) {
    for absent in [false, true] {
        let (backend, requests) = Backend::test_channel();
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
                assert_eq!(shell.view, View::Grid);
                if absent {
                    shell.panes[0].as_mut().unwrap().backend = None;
                }
                drop(requests);
            })
            .unwrap();
        frame(cx, handle);
        handle
            .update(cx, |shell, _, _| {
                assert_eq!(shell.view, View::List);
                let host = shell.panes[0].as_ref().unwrap();
                assert_eq!(host.pane.state.kind, crate::pane::Kind::Error);
                assert!(host.pane.requests.outstanding.is_none());
                assert!(shell.slots.states[0].is_some());
            })
            .unwrap();
    }
}

#[gpui::test]
fn u10_grid_switch_coalesced_with_rows_keeps_the_last_drawn_window(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let handle = cx.add_window(|_, cx| named_shell(backend, cx));
    handle
        .update(cx, |shell, window, cx| {
            shell.set_view(View::Grid, window, cx)
        })
        .unwrap();
    frame(cx, handle);
    {
        let visual = gpui::VisualTestContext::from_window(handle.into(), cx);
        visual.simulate_resize(gpui::size(gpui::px(500.), gpui::px(1440.)));
    }
    handle
        .update(cx, |shell, _, _| {
            shell.panes[0]
                .as_mut()
                .unwrap()
                .grid
                .as_mut()
                .unwrap()
                .view
                .thumb = super::geometry::ThumbSize::named("largest");
        })
        .unwrap();
    frame(cx, handle);
    key(cx, handle, "end");
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
            );
        })
        .unwrap();
    frame(cx, handle);
    handle
        .update(cx, |shell, _, _| {
            shell.panes[0].as_mut().unwrap().pane.cursor = 800
        })
        .unwrap();
    frame(cx, handle);
    handle
        .update(cx, |shell, window, cx| {
            let host = shell.panes[0].as_ref().unwrap();
            let ask = host.pane.requests.outstanding.unwrap();
            shell.receive(
                0,
                Event::Reply(protocol::Reply::Rows {
                    start: ask.start,
                    listing: ask.listing,
                    rows: (ask.start..(ask.start + ask.count).min(host.pane.held.total))
                        .map(|index| protocol::RowFacts {
                            name: format!("file-{index}.txt"),
                            ..Default::default()
                        })
                        .collect(),
                }),
                window,
                cx,
            );
            shell.set_view(View::List, window, cx);
        })
        .unwrap();
    frame(cx, handle);
    handle
        .update(cx, |shell, _, _| {
            assert_eq!(shell.view, View::Grid);
            let host = shell.panes[0].as_ref().unwrap();
            assert!(
                host.pane.held.start > 700,
                "a prior refusal must not roll back the later successful window"
            );
            let state = host.grid.as_ref().unwrap().state.borrow();
            assert!(state.drawn_row(0).unwrap().cursor);
            assert!(state.drawn().iter().all(|tile| tile.name().is_some()));
            assert!(host.pane.requests.outstanding.is_some());
        })
        .unwrap();
}
