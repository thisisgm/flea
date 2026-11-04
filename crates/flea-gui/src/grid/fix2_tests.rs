//! FIX2 regressions: the Grid row readers and a staged switch under a superseding listing.
use super::join_tests::{call, frame, named_shell};
use crate::{
    backend::{Backend, Event},
    window::startup::View,
};
use flea::protocol;
use gpui::{px, TestAppContext, VisualTestContext};

#[gpui::test]
fn u10_fix2_grid_visible_rows_divides_by_the_painted_file_row(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let handle = cx.add_window(|_, cx| named_shell(backend, cx));
    handle
        .update(cx, |shell, window, cx| {
            shell.set_view(View::Grid, window, cx)
        })
        .unwrap();
    frame(cx, handle);
    {
        let visual = VisualTestContext::from_window(handle.into(), cx);
        visual.simulate_resize(gpui::size(px(1200.), px(600.)));
    }
    handle
        .update(cx, |shell, _, cx| {
            let fonts = crate::presentation::Fonts::from_sources(
                "[font]\nbase-size=12",
                "",
                shell.active_typeface().font.family.to_string(),
            );
            assert_eq!((fonts.base, fonts.body, fonts.body_small), (12, 12, 11));
            shell.panes[0]
                .as_mut()
                .unwrap()
                .list
                .borrow_mut()
                .set_typeface(fonts.typeface(&flea::jsondoc::Json::Null));
            cx.notify();
        })
        .unwrap();
    frame(cx, handle);
    let file_row = handle
        .update(cx, |shell, _, _| {
            shell.panes[0]
                .as_ref()
                .unwrap()
                .grid
                .as_ref()
                .unwrap()
                .view
                .metrics
                .file_row
        })
        .unwrap();
    assert_eq!(file_row, 26., "body 12 paints a 26 px file row");
    let area: Vec<f32> = call(cx, handle, "listAreaRect", &[])
        .stdout
        .split_whitespace()
        .map(|value| value.parse().unwrap())
        .collect();
    let visible: usize = call(cx, handle, "visibleRows", &[])
        .stdout
        .trim()
        .parse()
        .unwrap();
    let expected = ((area[3] / file_row).ceil() as usize).max(1);
    assert_eq!(
        visible, expected,
        "v0.3.12 answers max(1, ceil(list.height / Theme.fileRowHeight)): {} px of area at a {} px painted file row, answered {}",
        area[3], file_row, visible
    );
}

#[gpui::test]
fn u10_fix2_a_staged_switch_waits_for_the_superseding_listings_complete_rows(
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
            assert_eq!(shell.view, View::Grid, "the switch is staged");
        })
        .unwrap();
    frame(cx, handle);
    let first = handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_mut().unwrap();
            assert!(host
                .pane
                .begin("/files".into(), crate::pane::swap::Options::default(), 0,));
            host.pane
                .swap
                .pending
                .as_ref()
                .expect("the reload is out")
                .first
        })
        .unwrap();
    frame(cx, handle);
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(
                shell.view,
                View::Grid,
                "the switch stays staged while the reload is out"
            );
            assert!(host
                .grid
                .as_ref()
                .is_some_and(|grid| grid.next_view.is_some()));
        })
        .unwrap();
    handle
        .update(cx, |shell, window, cx| {
            shell.receive(
                0,
                Event::Reply(protocol::Reply::Listed {
                    path: "/files".into(),
                    total: 1000,
                }),
                window,
                cx,
            );
            shell.receive(
                0,
                Event::Reply(protocol::Reply::Rows {
                    start: 0,
                    listing: 3,
                    rows: (0..first)
                        .map(|index| protocol::RowFacts {
                            name: format!("file-{index}.txt"),
                            ..Default::default()
                        })
                        .collect(),
                }),
                window,
                cx,
            );
            // The navigation's landing selects row 999, outside the new listing's first window.
            shell.panes[0].as_mut().unwrap().pane.cursor = 999;
            assert_eq!(
                shell.view,
                View::Grid,
                "the reload lands while the List switch is still staged"
            );
        })
        .unwrap();
    frame(cx, handle);
    handle
        .update(cx, |shell, _, _| {
            assert_eq!(
                shell.view,
                View::Grid,
                "the superseding listing does not land a half-drawn List"
            );
            assert!(
                shell.panes[0]
                    .as_ref()
                    .unwrap()
                    .pane
                    .requests
                    .outstanding
                    .is_some(),
                "the List viewport is fetched, never drawn partial"
            );
        })
        .unwrap();
    handle
        .update(cx, |shell, window, cx| {
            let host = shell.panes[0].as_ref().unwrap();
            let ask = host.pane.requests.outstanding.expect("the destination ask");
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
    handle
        .update(cx, |shell, _, _| {
            assert_eq!(shell.view, View::List);
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.pane.cursor, 999);
            let list = host.list.borrow();
            assert!(
                list.visible()
                    .iter()
                    .any(|row| row.index == 999 && row.prepared.is_some()),
                "the landing row is drawn by the List's first frame"
            );
            assert!(
                list.visible().iter().all(|row| row.prepared.is_some()),
                "the first List frame has no loading rows"
            );
        })
        .unwrap();
}
