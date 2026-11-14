//! Filter and search thumbnails follow listing identities through the production demand owner.
use super::*;
use crate::backend::Event;
use flea::{jsondoc::Json, protocol::Reply};

#[path = "pictures_sparse_tests.rs"]
mod sparse;

fn images(cx: &mut TestAppContext) -> (WindowHandle<Shell>, impl FnMut() -> Option<String>) {
    let (handle, lines) = open(cx, r#"{"preview":{"thumbnails":"images"}}"#);
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_mut().unwrap();
            let mut rows = host.pane.held.rows.clone();
            for (at, row) in rows.iter_mut().enumerate() {
                row.name = format!("photo-{at}.png");
                row.icon = "image-x-generic".into();
                row.dir = false;
                row.mode = 0o100644;
            }
            host.pane.held = Rc::new(Held {
                rows,
                ..*host.pane.held
            });
        })
        .unwrap();
    (handle, lines)
}

fn rows(lines: &[String], command: &str) -> Vec<usize> {
    lines
        .iter()
        .filter_map(|line| flea::jsondoc::parse(line).ok())
        .filter(|doc| doc.get("c").and_then(Json::as_str) == Some(command))
        .flat_map(|doc| {
            doc.get("rows")
                .and_then(Json::as_array)
                .unwrap()
                .iter()
                .map(|row| row.as_f64().unwrap() as usize)
                .collect::<Vec<_>>()
        })
        .collect()
}

fn filter(cx: &mut TestAppContext, handle: WindowHandle<Shell>, query: &str, rows: &[usize]) {
    handle
        .update(cx, |shell, _, cx| {
            shell.filter_update(0, |filter, pane| filter.typed(query, pane), cx);
        })
        .unwrap();
    filter_reply(cx, handle, rows);
}

fn filter_reply(cx: &mut TestAppContext, handle: WindowHandle<Shell>, rows: &[usize]) {
    handle
        .update(cx, |shell, _, cx| {
            let host = shell.panes[0].as_ref().unwrap();
            let id = host.filter.owner.asked().unwrap().id;
            let query = host.filter.owner.query();
            let rows = rows.iter().map(usize::to_string).collect::<Vec<_>>().join(",");
            let line = format!(
                r#"{{"t":"filtered","id":{id},"query":"{query}","n":{},"rows":[{rows}],"listing":{},"ms":0}}"#,
                host.pane.held.total, host.pane.held.listing
            );
            assert!(shell.filter_line(0, &line, cx));
        })
        .unwrap();
    frame(cx, handle);
}

fn search(cx: &mut TestAppContext, handle: WindowHandle<Shell>, query: &str, names: &[&str]) {
    handle
        .update(cx, |shell, window, cx| {
            shell.search_update(0, None, |search, host| search.open_query(query, host), window, cx);
            let generation = shell.panes[0].as_ref().unwrap().search.owner.session.gen as usize;
            let n = names.len();
            let listed = format!(r#"{{"t":"listed","path":"/files","n":{n},"gen":{generation}}}"#);
            shell.receive(0, Backend::test_reply_line(&listed), window, cx);
            let rows = names.iter().map(|name| RowFacts {
                name: (*name).into(), icon: "image-x-generic".into(), mode: 0o100644,
                size: 100, mtime: 1_700_000_000, ..Default::default()
            }).collect();
            shell.receive(0, Event::Session {
                generation, reply: Reply::Rows { start: 0, listing: generation + 10, rows },
            }, window, cx);
            let searched = format!(r#"{{"t":"searched","n":{n},"scanned":1000,"ms":1,"cancelled":false,"gen":{generation},"walking":false}}"#);
            shell.receive(0, Backend::test_reply_line(&searched), window, cx);
            assert!(shell.panes[0].as_ref().unwrap().pane.state.search);
        })
        .unwrap();
}

#[gpui::test]
fn fx477_grid_filter_asks_only_drawn_listing_rows_and_keeps_surviving_pictures(
    cx: &mut TestAppContext,
) {
    let (handle, mut lines) = images(cx);
    handle
        .update(cx, |shell, _, cx| {
            shell.filter_update(
                0,
                |filter, _| {
                    filter.start();
                    None
                },
                cx,
            );
        })
        .unwrap();
    filter(cx, handle, "a", &[6, 18, 31]);
    let asked = first_settle(cx, handle, &mut lines);
    eprintln!(
        "FX477 filter-a: {:?}",
        read(cx, handle, |shell| pictures(shell).counters())
    );
    assert_eq!(
        rows(&asked, "thumb"),
        [6, 18, 31],
        "view positions are not listing rows"
    );
    for row in [6, 18, 31] {
        deliver(cx, handle, &thumbed(row, PICTURE));
    }
    cx.run_until_parked();
    step(cx, handle);
    let before = read(cx, handle, |shell| pictures(shell).counters());
    filter(cx, handle, "x", &[18, 31]);
    for row in [18, 31] {
        assert!(
            read(cx, handle, |shell| drawn(shell, row).1).is_some(),
            "surviving row {row} flickered"
        );
    }
    settle(cx, handle, 120);
    assert!(rows(&drained(&mut lines), "thumb").is_empty());
    assert_eq!(read(cx, handle, |shell| pictures(shell).counters()), before);
    eprintln!("FX477 filter-ax: {before:?}");
}

#[gpui::test]
fn fx477_late_answer_for_a_removed_match_never_starts_a_decode(cx: &mut TestAppContext) {
    let (handle, mut lines) = images(cx);
    filter(cx, handle, "a", &[6, 18, 31]);
    assert_eq!(
        rows(&first_settle(cx, handle, &mut lines), "thumb"),
        [6, 18, 31]
    );
    deliver(cx, handle, &thumbed(18, PICTURE));
    cx.run_until_parked();
    filter(cx, handle, "x", &[18, 31]);
    deliver(cx, handle, &thumbed(6, PICTURE));
    cx.run_until_parked();
    settle(cx, handle, 120);
    // Row 6's one answer ended its job, so the settle neither cancels nor asks it.
    let after = drained(&mut lines);
    assert!(rows(&after, "thumbcancel").is_empty(), "{after:?}");
    assert!(rows(&after, "thumb").is_empty(), "{after:?}");
    let counters = read(cx, handle, |shell| pictures(shell).counters());
    assert_eq!(
        (
            counters.asked,
            counters.cancelled,
            counters.answered,
            counters.decoded
        ),
        (3, 0, 1, 1)
    );
    assert!(read(cx, handle, |shell| drawn(shell, 18).1).is_some());
    eprintln!("FX477 removed-match-late-answer: {counters:?}");
}

#[gpui::test]
fn fx477_empty_grid_filter_cancels_pending_rows_without_asking_removed_files(
    cx: &mut TestAppContext,
) {
    let (handle, mut lines) = images(cx);
    first_settle(cx, handle, &mut lines);
    let pending = read(cx, handle, |shell| pictures(shell).counters().asked);
    assert!(pending > 0);
    handle
        .update(cx, |shell, _, cx| {
            shell.filter_update(
                0,
                |filter, _| {
                    filter.start();
                    None
                },
                cx,
            );
        })
        .unwrap();
    filter(cx, handle, "absent", &[]);
    settle(cx, handle, 120);
    let lines = drained(&mut lines);
    assert!(rows(&lines, "thumb").is_empty());
    assert_eq!(rows(&lines, "thumbcancel").len(), pending);
    let counters = read(cx, handle, |shell| pictures(shell).counters());
    assert_eq!(
        (counters.asked, counters.cancelled, counters.decoded),
        (pending, pending, 0)
    );
    eprintln!("FX477 filter-empty: {counters:?}");
}

#[gpui::test]
fn fx477_filtered_grid_scroll_requests_the_drawn_files_and_cancels_only_departed_rows(
    cx: &mut TestAppContext,
) {
    let (handle, mut lines) = images(cx);
    let shown = (0..160).map(|at| at * 2).collect::<Vec<_>>();
    filter(cx, handle, "a", &shown);
    let first = rows(&first_settle(cx, handle, &mut lines), "thumb");
    handle
        .update(cx, |shell, _, _| {
            shell.panes[0]
                .as_mut()
                .unwrap()
                .grid
                .as_mut()
                .unwrap()
                .view
                .scroll = 600.;
        })
        .unwrap();
    step(cx, handle);
    let drawn = read(cx, handle, |shell| {
        let host = shell.panes[0].as_ref().unwrap();
        let state = host.grid.as_ref().unwrap().state.borrow();
        state
            .drawn()
            .iter()
            .map(|row| row.listing)
            .collect::<Vec<_>>()
    });
    settle(cx, handle, 120);
    let moved = drained(&mut lines);
    assert_ne!(drawn, first);
    assert!(!rows(&moved, "thumb").is_empty());
    assert!(!rows(&moved, "thumbcancel").is_empty());
    assert_eq!(
        rows(&moved, "thumb"),
        drawn
            .iter()
            .copied()
            .filter(|row| !first.contains(row))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        rows(&moved, "thumbcancel"),
        first
            .iter()
            .copied()
            .filter(|row| !drawn.contains(row))
            .collect::<Vec<_>>()
    );
    assert!(rows(&moved, "thumb").iter().all(|row| shown.contains(row)));
    eprintln!(
        "FX477 filter-scroll: {:?}",
        read(cx, handle, |shell| pictures(shell).counters())
    );
}

#[gpui::test]
fn fx477_search_strip_asks_and_draws_thumbnails_in_each_listing_view(cx: &mut TestAppContext) {
    for view in [View::Grid, View::List] {
        let (handle, mut lines) = images(cx);
        handle
            .update(cx, |shell, window, cx| {
                shell.set_view(view, window, cx);
                shell.search_action("search", window, cx);
            })
            .unwrap();
        search(cx, handle, "photo", &["photo-18.png", "sub/photo-31.png"]);
        let asked = first_settle(cx, handle, &mut lines);
        assert_eq!(
            rows(&asked, "thumb"),
            [0, 1],
            "{view:?}: the strip's results own thumbnails"
        );
        for row in 0..2 {
            deliver(cx, handle, &thumbed(row, PICTURE));
        }
        cx.run_until_parked();
        step(cx, handle);
        read(cx, handle, |shell| {
            let host = shell.panes[0].as_ref().unwrap();
            let pictures = &host.list.borrow().pictures;
            for row in 0..2 {
                assert!(pictures.ready(&host.pane.held, row), "{view:?} row {row}");
            }
            assert_eq!(pictures.counters().decoded, 2);
            eprintln!("FX477 search-{view:?}: {:?}", pictures.counters());
        });
        search(cx, handle, "31", &["sub/photo-31.png"]);
        step(cx, handle);
        read(cx, handle, |shell| {
            let host = shell.panes[0].as_ref().unwrap();
            assert!(
                host.list.borrow().pictures.ready(&host.pane.held, 0),
                "{view:?}: a surviving file keeps its picture at its new row"
            );
        });
        settle(cx, handle, 120);
        assert!(
            rows(&drained(&mut lines), "thumb").is_empty(),
            "{view:?}: a carried picture is never decoded again"
        );
    }
}

#[gpui::test]
fn fx477_columns_plan_the_current_filter_before_the_next_frame_is_painted(cx: &mut TestAppContext) {
    let (handle, mut lines) = images(cx);
    handle
        .update(cx, |shell, window, cx| {
            shell.set_view(View::Columns, window, cx)
        })
        .unwrap();
    frame(cx, handle);
    deliver(cx, handle, LOCAL);
    for _ in 0..3 {
        frame(cx, handle);
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(120));
        cx.run_until_parked();
    }
    let initial = drained(&mut lines);
    let mut waiting = rows(&initial, "thumb");
    let cancelled = rows(&initial, "thumbcancel");
    waiting.retain(|row| !cancelled.contains(row));
    assert!(!waiting.is_empty());
    filter(cx, handle, "a", &[6, 18, 31]);
    frame(cx, handle);
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(120));
    cx.run_until_parked();
    let changed = drained(&mut lines);
    let cancelled = rows(&changed, "thumbcancel");
    waiting.retain(|row| !cancelled.contains(row));
    waiting.extend(rows(&changed, "thumb"));
    waiting.sort_unstable();
    assert_eq!(
        waiting,
        [6, 18, 31],
        "thumbnail demand cannot wait for another painted frame"
    );
}

#[path = "pictures_carry_tests.rs"]
mod carried;
