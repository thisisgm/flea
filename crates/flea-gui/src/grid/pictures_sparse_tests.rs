//! Sparse windows have one dense window's reach and retain only drawn matches.
use super::*;

#[path = "pictures_ranked_tests.rs"]
mod ranked;

fn open_sparse(cx: &mut TestAppContext) -> (WindowHandle<Shell>, impl FnMut() -> Option<String>) {
    let (handle, _lines) = images(cx);
    let (backend, lines) = Backend::test_window_lines();
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_mut().unwrap();
            host.backend = Some(backend);
            let pane = &mut host.pane;
            pane.requests.cancel();
            pane.held = Rc::new(Held {
                total: 3000,
                rows: pane.held.rows.clone(),
                ..*pane.held
            });
        })
        .unwrap();
    (handle, lines)
}

fn answer(cx: &mut TestAppContext, handle: WindowHandle<Shell>, start: usize, count: usize) {
    handle
        .update(cx, |shell, window, cx| {
            shell.receive(
                0,
                Event::Reply(Reply::Rows {
                    start,
                    listing: 2,
                    rows: (start..start + count)
                        .map(|row| RowFacts {
                            name: format!("photo-{row}.png"),
                            icon: "image-x-generic".into(),
                            mode: 0o100644,
                            size: 100,
                            mtime: 1_700_000_000,
                            ..Default::default()
                        })
                        .collect(),
                }),
                window,
                cx,
            );
        })
        .unwrap();
}

#[gpui::test]
fn fx477_sparse_filter_fetches_all_visible_facts_and_then_asks_each_thumbnail_once(
    cx: &mut TestAppContext,
) {
    let (handle, mut lines) = open_sparse(cx);
    let wanted = (0..50).map(|at| at * 60).collect::<Vec<_>>();
    filter(cx, handle, "ax", &wanted);
    let mut fetched = Vec::new();
    for _ in 0..50 {
        step(cx, handle);
        let requests = drained(&mut lines)
            .into_iter()
            .filter_map(|line| flea::jsondoc::parse(&line).ok())
            .filter(|doc| doc.get("c").and_then(Json::as_str) == Some("window"))
            .collect::<Vec<_>>();
        if requests.is_empty() {
            break;
        }
        for request in requests {
            let number = |key| request.get(key).and_then(Json::as_f64).unwrap() as usize;
            let (start, count) = (number("start"), number("count"));
            assert!(
                count <= wanted.len() + 2 * crate::pane::requests::BUFFER,
                "a sparse window reaches no further than a dense one"
            );
            assert!(wanted.contains(&start) && wanted.contains(&(start + count - 1)));
            fetched.push(start);
            answer(cx, handle, start, count);
        }
    }
    assert_eq!(
        fetched,
        [360, 720, 1080, 1440, 1800, 2160, 2520, 2880],
        "the six already-held matches need no stat, and each window covers the next missing matches"
    );
    read(cx, handle, |shell| {
        let held = &shell.panes[0].as_ref().unwrap().pane.held;
        assert_eq!(held.rows.len(), 50, "facts stay scoped to drawn files");
        assert!(wanted.iter().all(|&row| held.row(row).is_some()));
        assert_eq!(held.index_at(49), 2940);
        assert_eq!(
            crate::navigation::anchor::index_of(
                &shell.panes[0].as_ref().unwrap().pane,
                "photo-2940.png"
            ),
            Some(2940)
        );
    });
    let asked = first_settle(cx, handle, &mut lines);
    assert_eq!(rows(&asked, "thumb"), wanted);
    for row in &wanted {
        deliver(cx, handle, &thumbed(*row, PICTURE));
    }
    cx.run_until_parked();
    step(cx, handle);
    for row in &wanted {
        assert!(read(cx, handle, |shell| drawn(shell, *row).1).is_some());
    }
    let before = read(cx, handle, |shell| pictures(shell).counters());
    settle(cx, handle, 120);
    assert!(rows(&drained(&mut lines), "thumb").is_empty());
    assert_eq!(read(cx, handle, |shell| pictures(shell).counters()), before);
    eprintln!("FX477 sparse-filter: fetched={}, {before:?}", fetched.len());
}

#[gpui::test]
fn fx477_late_metadata_keeps_the_current_querys_already_visible_picture(cx: &mut TestAppContext) {
    let (handle, mut lines) = open_sparse(cx);
    first_settle(cx, handle, &mut lines);
    deliver(cx, handle, &thumbed(6, PICTURE));
    cx.run_until_parked();
    filter(
        cx,
        handle,
        "ax",
        &(0..50).map(|at| at * 60).collect::<Vec<_>>(),
    );
    read(cx, handle, |shell| {
        assert_eq!(
            shell.panes[0]
                .as_ref()
                .unwrap()
                .pane
                .requests
                .outstanding
                .map(|request| (request.start, request.count)),
            Some((360, 301))
        );
    });
    handle
        .update(cx, |shell, _, cx| {
            shell.filter_update(0, |filter, pane| filter.backspace(pane), cx);
        })
        .unwrap();
    filter_reply(cx, handle, &(0..500).map(|at| at * 6).collect::<Vec<_>>());
    assert!(read(cx, handle, |shell| drawn(shell, 6).1).is_some());
    answer(cx, handle, 360, 301);
    step(cx, handle);
    assert!(
        read(cx, handle, |shell| drawn(shell, 6).1).is_some(),
        "an older metadata reply cannot erase current visible facts"
    );
    assert_eq!(
        read(cx, handle, |shell| pictures(shell).counters().decoded),
        1
    );
}

#[gpui::test]
fn fx477_metadata_reply_after_an_empty_filter_cannot_repopulate_its_rows(cx: &mut TestAppContext) {
    let (handle, _lines) = open_sparse(cx);
    filter(
        cx,
        handle,
        "ax",
        &(0..50).map(|at| at * 60).collect::<Vec<_>>(),
    );
    filter(cx, handle, "z", &[]);
    let held = read(cx, handle, |shell| {
        Rc::clone(&shell.panes[0].as_ref().unwrap().pane.held)
    });
    answer(cx, handle, 360, 301);
    step(cx, handle);
    read(cx, handle, |shell| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.filter.shown.as_ref().unwrap().len(), 0);
        assert!(host.pane.requests.outstanding.is_none());
        assert_eq!(host.pane.held.rows, held.rows);
        assert_eq!(host.pane.held.listing, held.listing);
        assert!(host.pane.held.row(360).is_none());
        assert_eq!(
            host.drawn_view, None,
            "the empty-results surface replaces the retained Grid frame"
        );
    });
}

#[gpui::test]
fn fx477_filtered_wheel_stops_at_the_drawn_end_and_fetches_its_facts(cx: &mut TestAppContext) {
    let (handle, mut lines) = open_sparse(cx);
    let matches = (0..500).map(|at| at * 6).collect::<Vec<_>>();
    filter(cx, handle, "a", &matches);
    deliver(cx, handle, LOCAL);
    handle
        .update(cx, |shell, window, cx| {
            let at = shell.panes[0]
                .as_ref()
                .unwrap()
                .grid
                .as_ref()
                .unwrap()
                .state
                .borrow()
                .bounds
                .center();
            for _ in 0..24 {
                shell.wheel(
                    &gpui::ScrollWheelEvent {
                        position: at,
                        delta: gpui::ScrollDelta::Lines(gpui::point(0., -3.)),
                        ..Default::default()
                    },
                    window,
                    cx,
                );
            }
        })
        .unwrap();
    settle(cx, handle, 200);
    let wanted = read(cx, handle, |shell| {
        let host = shell.panes[0].as_ref().unwrap();
        let grid = host.grid.as_ref().unwrap();
        let layout = grid.view.layout(crate::grid::Shown::Only(&matches));
        assert_eq!(
            grid.view.scroll,
            layout.clamp(f32::MAX),
            "wheel targets the filtered end"
        );
        assert_eq!(
            grid.state.borrow().scroll,
            grid.view.drawn_scroll(),
            "facts and paint use the same scroll"
        );
        matches[layout.drawn(grid.view.drawn_scroll())].to_vec()
    });
    let mut asked = Vec::new();
    for _ in 0..200 {
        step(cx, handle);
        let messages = drained(&mut lines);
        asked.extend(rows(&messages, "thumb"));
        let requests = messages
            .iter()
            .filter_map(|line| flea::jsondoc::parse(line).ok())
            .filter(|doc| doc.get("c").and_then(Json::as_str) == Some("window"))
            .collect::<Vec<_>>();
        if requests.is_empty() {
            break;
        }
        for request in requests {
            let number = |key| request.get(key).and_then(Json::as_f64).unwrap() as usize;
            answer(cx, handle, number("start"), number("count"));
        }
    }
    settle(cx, handle, 120);
    asked.extend(rows(&drained(&mut lines), "thumb"));
    read(cx, handle, |shell| {
        let held = &shell.panes[0].as_ref().unwrap().pane.held;
        assert!(!wanted.is_empty());
        assert!(
            wanted.iter().all(|&row| held.row(row).is_some()),
            "every final tile has facts"
        );
    });
    for &row in &wanted {
        assert!(asked.contains(&row), "the final viewport asks for {row}");
        deliver(cx, handle, &thumbed(row, PICTURE));
    }
    cx.run_until_parked();
    step(cx, handle);
    for row in wanted {
        assert!(
            read(cx, handle, |shell| drawn(shell, row).1).is_some(),
            "the final viewport draws {row}"
        );
    }
}

#[gpui::test]
fn fx477_grid_clear_refills_indexed_facts_even_when_their_count_covers_the_viewport(
    cx: &mut TestAppContext,
) {
    let (handle, _lines) = open_sparse(cx);
    handle
        .update(cx, |shell, _, _| {
            let pane = &mut shell.panes[0].as_mut().unwrap().pane;
            let mut rows = pane.held.rows.clone();
            for (at, row) in rows.iter_mut().enumerate() {
                row.listing_index = Some(at * 6);
                row.name = format!("photo-{}.png", at * 6);
            }
            pane.held = Rc::new(Held { rows, ..*pane.held });
            assert!(pane.held.row(1).is_none());
        })
        .unwrap();
    step(cx, handle);
    read(cx, handle, |shell| {
        let host = shell.panes[0].as_ref().unwrap();
        let grid = host.grid.as_ref().unwrap();
        let layout = grid.view.layout(crate::grid::Shown::All(3000));
        assert!(host.pane.held.rows.len() > layout.drawn(0.).len());
        let request = host
            .pane
            .requests
            .outstanding
            .expect("clearing sparse facts needs a dense refill");
        assert_eq!(request.start, 0);
        assert!(request.count >= layout.drawn(0.).len());
    });
}
