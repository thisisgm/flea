//! Close matches retain ranking while metadata and picture membership use listing identities.
use super::*;

fn close(cx: &mut TestAppContext, handle: WindowHandle<Shell>, matches: &[usize]) {
    handle.update(cx, |shell, _, cx| {
        shell.filter_update(0, |filter, pane| filter.typed("rank", pane), cx);
        let host = shell.panes[0].as_ref().unwrap();
        let id = host.filter.owner.asked().unwrap().id;
        let close = matches.iter().map(usize::to_string).collect::<Vec<_>>().join(",");
        let line = format!(r#"{{"t":"filtered","id":{id},"query":"rank","n":3000,"rows":[],"close":[{close}],"listing":2,"ms":0}}"#);
        assert!(shell.filter_line(0, &line, cx));
    }).unwrap();
    frame(cx, handle);
}

fn ranked_pictures(cx: &mut TestAppContext, view: View) {
    let (handle, mut lines) = open_sparse(cx);
    handle
        .update(cx, |shell, window, cx| shell.set_view(view, window, cx))
        .unwrap();
    let wanted = [2880, 60, 1500, 300];
    close(cx, handle, &wanted);
    let mut fetched = Vec::new();
    for _ in 0..8 {
        step(cx, handle);
        let request = read(cx, handle, |shell| {
            shell.panes[0].as_ref().unwrap().pane.requests.outstanding
        });
        let Some(request) = request else { break };
        assert!(request.count <= wanted.len() + 2 * crate::pane::requests::BUFFER);
        assert!(
            !fetched.contains(&request.start),
            "no repeated metadata request"
        );
        fetched.push(request.start);
        answer(cx, handle, request.start, request.count);
    }
    assert!(!fetched.is_empty());
    read(cx, handle, |shell| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.filter.shown.as_deref(), Some(wanted.as_slice()));
        assert!(wanted.iter().all(|&row| host.pane.held.row(row).is_some()));
        assert!(host.pane.requests.outstanding.is_none());
        assert!(host
            .pane
            .held
            .rows
            .windows(2)
            .all(|rows| rows[0].listing_index < rows[1].listing_index));
    });
    let mut asked = rows(&first_settle(cx, handle, &mut lines), "thumb");
    asked.sort_unstable();
    assert_eq!(
        asked,
        [60, 300, 1500, 2880],
        "{view:?}: one ask per drawn row"
    );
    for row in wanted {
        deliver(cx, handle, &thumbed(row, PICTURE));
    }
    cx.run_until_parked();
    step(cx, handle);
    read(cx, handle, |shell| {
        let host = shell.panes[0].as_ref().unwrap();
        let list = host.list.borrow();
        for row in wanted {
            assert!(
                list.pictures.ready(&host.pane.held, row),
                "{view:?}: picture for {row}"
            );
            if view == View::Grid {
                assert!(drawn(shell, row).1.is_some());
            }
        }
        assert_eq!(list.pictures.counters().decoded, wanted.len());
    });
    settle(cx, handle, 120);
    let repeated = drained(&mut lines);
    assert!(rows(&repeated, "thumb").is_empty());
    assert!(!repeated
        .iter()
        .any(|line| line.contains("\"c\":\"window\"")));
}

#[gpui::test]
fn fx477_ranked_close_matches_grid(cx: &mut TestAppContext) {
    ranked_pictures(cx, View::Grid);
}

#[gpui::test]
fn fx477_ranked_close_matches_list(cx: &mut TestAppContext) {
    ranked_pictures(cx, View::List);
}

#[test]
fn fx477_sparse_requests_sort_and_deduplicate_wanted_rows() {
    let mut requests = crate::pane::requests::Requests::default();
    let held = Held {
        listing: 1,
        total: 100,
        ..Default::default()
    };
    let request = requests.wanted_sparse(&held, &[70, 5, 70]).unwrap();
    assert_eq!((request.start, request.count), (5, 66));
}

#[gpui::test]
fn fx477_filtered_grid_transition_fetches_the_destination_shown_rows_before_landing(
    cx: &mut TestAppContext,
) {
    let mut clipped = Vec::new();
    for view in [View::List, View::Columns] {
        for one_row in [false, true] {
            for cursor_at in [0, 60, 79] {
                let (handle, _lines) = open_sparse(cx);
                handle
                    .update(cx, |shell, _, cx| {
                        shell.filter_update(0, |filter, pane| filter.typed("rank", pane), cx);
                    })
                    .unwrap();
                if one_row {
                    let size = handle
                        .update(cx, |shell, window, _| {
                            let size = window.viewport_size();
                            let height = shell
                                .geometry_for(view, window)
                                .content_height(View::List, shell.slots.strip_height);
                            let row = shell.panes[0].as_ref().unwrap().pane.row_height();
                            gpui::size(size.width, gpui::px(f32::from(size.height) - height + row))
                        })
                        .unwrap();
                    cx.simulate_window_resize(handle.into(), size);
                    frame(cx, handle);
                }
                let wanted = (0..80).map(|at| 600 + at * 30).collect::<Vec<_>>();
                let cursor = wanted[cursor_at];
                // Do not paint between the real filter reply and the switch: its destination owns the first sparse request.
                handle.update(cx, |shell, window, cx| {
                    let id = shell.panes[0].as_ref().unwrap().filter.owner.asked().unwrap().id;
                    let rows = wanted.iter().map(usize::to_string).collect::<Vec<_>>().join(",");
                    assert!(shell.filter_line(0, &format!(r#"{{"t":"filtered","id":{id},"query":"rank","n":3000,"rows":[{rows}],"listing":2,"ms":0}}"#), cx));
                    shell.panes[0].as_mut().unwrap().pane.cursor = cursor;
                    shell.set_view(view, window, cx);
                    assert_eq!(shell.view, View::Grid, "missing shown facts defer {view:?}");
                    shell.finish_grid_view(window, cx);
                    let pane = &shell.panes[0].as_ref().unwrap().pane;
                    let drawn = pane.requests.sparse.as_ref().unwrap();
                    assert!(drawn.len() < wanted.len());
                    let first = wanted.iter().position(|row| row == &drawn[0]).unwrap();
                    assert_eq!(drawn, &wanted[first..first + drawn.len()]);
                    assert!(drawn.contains(&cursor));
                    let request = pane.requests.outstanding.unwrap();
                    assert_eq!(request.start, drawn[0]);
                    assert!(request.count <= drawn.len() + 2 * crate::pane::requests::BUFFER);
                }).unwrap();
                let mut fetched = Vec::new();
                for _ in 0..20 {
                    let request = read(cx, handle, |shell| {
                        assert_eq!(shell.view, View::Grid);
                        shell.panes[0]
                            .as_ref()
                            .unwrap()
                            .pane
                            .requests
                            .outstanding
                            .unwrap()
                    });
                    assert!(!fetched.contains(&request.start));
                    fetched.push(request.start);
                    answer(cx, handle, request.start, request.count);
                    frame(cx, handle);
                    if read(cx, handle, |shell| shell.view) == view {
                        break;
                    }
                }
                frame(cx, handle);
                let (whole, held) = read(cx, handle, |shell| {
                    assert_eq!(shell.view, view);
                    assert_eq!(shell.shown_view(), View::List);
                    let pane = &shell.panes[0].as_ref().unwrap().pane;
                    assert_eq!(pane.cursor, cursor);
                    let whole = landed_rows(pane, &wanted, cursor_at);
                    if one_row {
                        assert_eq!(pane.viewport_height(), pane.row_height(), "{view:?}");
                    }
                    eprintln!("FX477 landing {view:?} cursor={cursor_at} one_row={one_row} whole={whole} height={} row={} scroll={}", pane.viewport_height(), pane.row_height(), pane.scroll_y);
                    (whole, Rc::clone(&pane.held))
                });
                if !whole {
                    clipped.push((view, cursor_at, one_row));
                }
                if view == View::Columns {
                    handle
                        .update(cx, |shell, window, cx| {
                            crate::window::columns_tests::host::settle(shell, window, cx);
                        })
                        .unwrap();
                    frame(cx, handle);
                    read(cx, handle, |shell| {
                        assert_eq!(
                            shell.shown_view(),
                            View::Columns,
                            "track replies end the entry"
                        );
                        let host = shell.panes[0].as_ref().unwrap();
                        let pane = &host.pane;
                        assert!(
                            Rc::ptr_eq(&held, &pane.held),
                            "grow uses the prefetched snapshot"
                        );
                        assert!(
                            pane.requests.outstanding.is_none(),
                            "grow needs no later facts"
                        );
                        assert!(
                            landed_rows(pane, &wanted, cursor_at),
                            "whole cursor after grow"
                        );
                        let columns = host.columns.as_ref().unwrap();
                        let paint = columns
                            .painters
                            .get(crate::columns::geometry::Slot::Active)
                            .unwrap()
                            .borrow();
                        let drawn = paint.drawn.as_ref().unwrap();
                        assert!(Rc::ptr_eq(drawn.held.as_ref().unwrap(), &held));
                        let height = f32::from(drawn.bounds.size.height);
                        let row = drawn.row_height;
                        assert!(cursor_at as f32 * row + 0.01 >= drawn.scroll);
                        assert!((cursor_at + 1) as f32 * row <= drawn.scroll + height + 0.01);
                        let first = (drawn.scroll / row).floor() as usize;
                        let end =
                            (((drawn.scroll + height) / row).ceil() as usize).min(wanted.len());
                        for (position, &expected) in wanted.iter().enumerate().take(end).skip(first)
                        {
                            let listing = drawn.row_at(position).unwrap();
                            assert_eq!(listing, expected);
                            assert!(
                                held.row(listing).is_some(),
                                "drawn row {listing} was prefetched"
                            );
                        }
                        eprintln!("FX477 grown cursor={cursor_at} one_row={one_row} height={} row={} scroll={} drawn={first}..{end}", pane.viewport_height(), pane.row_height(), pane.scroll_y);
                    });
                }
            }
        }
    }
    assert!(clipped.is_empty(), "whole cursor row clipped: {clipped:?}");
}

fn landed_rows(pane: &crate::pane::Pane, wanted: &[usize], cursor_at: usize) -> bool {
    let row = pane.row_height();
    let scroll = pane.scroll_y;
    let height = pane.viewport_height();
    let first = (scroll / row).floor() as usize;
    let end = (((scroll + height) / row).ceil() as usize).min(wanted.len());
    assert!(wanted[first..end]
        .iter()
        .all(|&index| pane.held.row(index).is_some()));
    eprintln!(
        "FX477 held cursor={} visible={first}..{end} listing={:?}",
        wanted[cursor_at],
        &wanted[first..end]
    );
    cursor_at as f32 * row + 0.01 >= scroll
        && (cursor_at + 1) as f32 * row <= scroll + height + 0.01
}

#[gpui::test]
fn fx477_same_folder_relist_keeps_drawn_answers_live(cx: &mut TestAppContext) {
    let (handle, mut lines) = images(cx);
    let asked = rows(&first_settle(cx, handle, &mut lines), "thumb");
    let row = asked[0];
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_mut().unwrap();
            assert!(host.pane.swap.begin(
                host.path.clone(),
                0,
                &host.pane.state,
                crate::pane::swap::Options {
                    in_place: true,
                    ..Default::default()
                },
                1
            ));
            assert!(host.pane.busy());
        })
        .unwrap();
    frame(cx, handle);
    deliver(cx, handle, &thumbed(row, PICTURE));
    cx.run_until_parked();
    read(cx, handle, |shell| {
        assert_eq!(pictures(shell).counters().answered, 1);
        assert_eq!(pictures(shell).counters().decoded, 1);
    });
}
