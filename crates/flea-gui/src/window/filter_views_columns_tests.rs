//! A standing filter in Columns: the active track draws only its matches (EY-10), and no match keeps the columns (EY-12).
use super::{filter, footer};
use crate::{
    backend::Backend,
    columns::geometry::Slot,
    state::Shell,
    visual::geometry::Probe,
    window::{columns_tests::host, startup::View},
};
use flea::jsondoc::Json;
use gpui::{Context, VisualTestContext, Window};

fn active_rows(shell: &Shell) -> Vec<usize> {
    let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
    columns
        .painters
        .get(Slot::Active)
        .expect("the active track painted")
        .borrow()
        .visible()
        .collect()
}

#[test]
fn ey10_columns_draw_only_the_filtered_rows() {
    let mut cx = host::context();
    let (window, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, window, |shell, window, cx| {
        host::settle(shell, window, cx)
    });
    host::frame(&mut cx, window);
    assert_eq!(
        window
            .update(&mut cx, |shell, _, _| active_rows(shell))
            .unwrap(),
        vec![0, 1]
    );
    host::change(&mut cx, window, |shell, _, cx| {
        filter(shell, "d02", 2, 3, "1", cx)
    });
    host::frame(&mut cx, window);
    window
        .update(&mut cx, |shell, _, cx| {
            assert_eq!(
                active_rows(shell),
                vec![1],
                "HIG cols-filter: the column keeps only its match"
            );
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.pane.cursor, 1, "the cursor takes the row standing");
            let columns = host.columns.as_ref().unwrap();
            let paint = columns.painters.get(Slot::Active).unwrap().borrow();
            let drawn = paint.drawn.as_ref().unwrap();
            assert_eq!(
                (drawn.row_at(0), drawn.position_of(1), drawn.position_of(0)),
                (Some(1), Some(0), None)
            );
            assert_eq!(footer(shell, cx).0, "1 of 2 shown");
        })
        .unwrap();
}

#[test]
fn ey10_ey12_columns_keep_their_columns_under_the_no_match_headline() {
    let mut cx = host::context();
    let (window, _requests) = host::open_at(&mut cx, "/srv/a/b/c", 2100., 4);
    host::change(&mut cx, window, |shell, window, cx| {
        host::settle(shell, window, cx)
    });
    host::frame(&mut cx, window);
    host::change(&mut cx, window, |shell, _, cx| {
        filter(shell, "zzz", 2, 3, "", cx)
    });
    host::frame(&mut cx, window);
    let (pane, active, parents) = window
        .update(&mut cx, |shell, window, _| {
            assert_eq!(shell.view, View::Columns);
            assert!(
                active_rows(shell).is_empty(),
                "the active column draws no row"
            );
            let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
            let parents = columns
                .area
                .tracks()
                .iter()
                .filter(|track| track.slot.index() < Slot::Active.index())
                .count();
            (
                shell.geometry(window).panes[0],
                columns.area.layout.active_x(),
                parents,
            )
        })
        .unwrap();
    assert!(parents > 0, "the columns before the active one stay");
    let mut visual = VisualTestContext::from_window(window.into(), &cx);
    let slot = visual
        .debug_bounds("columns-no-match")
        .expect("HIG cols4-hero-slash: the headline stands in the columns");
    assert!(visual.debug_bounds("no-match-hero").is_some());
    assert_eq!(
        f32::from(slot.origin.x),
        pane.x + active,
        "four columns put the headline in the active span"
    );
    assert_eq!(f32::from(slot.size.width), pane.width - active);
}

/// The ids of every role the capture probe reads from the frame drawn last.
fn probed(shell: &Shell, window: &Window) -> Vec<String> {
    let probe = Probe::default();
    shell.visual_view_geometry(&probe, window);
    let data = probe.json(&Json::Null);
    data.get("roles")
        .and_then(Json::as_array)
        .unwrap()
        .iter()
        .filter_map(|role| role.get("id").and_then(Json::as_str).map(str::to_owned))
        .collect()
}

#[test]
fn ey12_capture_reads_no_track_the_no_match_headline_replaced() {
    let mut cx = host::context();
    let (window, _requests) = host::open_at(&mut cx, "/srv/a/b/c", 2100., 4);
    host::change(&mut cx, window, |shell, window, cx| {
        host::settle(shell, window, cx)
    });
    host::frame(&mut cx, window);
    let child = format!("columns-0-{}", Slot::Child.index());
    let before = window
        .update(&mut cx, |shell, window, _| probed(shell, window))
        .unwrap();
    assert!(
        before.iter().any(|id| id.starts_with(&child)),
        "the child track painted before the filter: {before:?}"
    );
    host::change(&mut cx, window, |shell, _, cx| {
        filter(shell, "zzz", 2, 3, "", cx)
    });
    host::frame(&mut cx, window);
    window
        .update(&mut cx, |shell, window, _| {
            let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
            assert!(
                columns
                    .painters
                    .get(Slot::Child)
                    .is_some_and(|paint| paint.borrow().drawn.is_some()),
                "the replaced track still holds the frame it drew before the filter"
            );
            let after = probed(shell, window);
            assert!(
                after.iter().all(|id| !id.starts_with(&child)),
                "HIG cols-filter-nomatch: nothing stands right of the active column: {after:?}"
            );
            let active = format!("columns-0-{}", Slot::Active.index());
            assert!(after.iter().any(|id| id.starts_with(&active)));
        })
        .unwrap();
}

fn line(shell: &mut Shell, text: &str, window: &mut Window, cx: &mut Context<Shell>) {
    shell.receive(0, Backend::test_reply_line(text), window, cx);
}

#[test]
fn ey12_columns_search_that_finds_nothing_keeps_the_columns() {
    let mut cx = host::context();
    let (window, _requests) = host::open_at(&mut cx, "/srv/a/b/c", 2100., 4);
    host::change(&mut cx, window, |shell, window, cx| {
        host::settle(shell, window, cx)
    });
    host::frame(&mut cx, window);
    host::change(&mut cx, window, |shell, window, cx| {
        assert!(shell.search_action("search", window, cx));
        assert_eq!(
            shell.view,
            View::Columns,
            "HIG cols4-hero-search: search stays in Columns"
        );
        for key in ["z", "q", "q"] {
            let keystroke = gpui::Keystroke {
                key: key.into(),
                key_char: Some(key.into()),
                ..Default::default()
            };
            shell.key(
                &gpui::KeyDownEvent {
                    keystroke,
                    is_held: false,
                    prefer_character_input: false,
                },
                window,
                cx,
            );
        }
        let generation = shell.panes[0].as_ref().unwrap().search.owner.session.gen as usize;
        line(
            shell,
            &format!(r#"{{"t":"listed","path":"/srv/a/b/c","n":0,"gen":{generation}}}"#),
            window,
            cx,
        );
        let rows = flea::protocol::Reply::Rows {
            start: 0,
            listing: 10 + generation,
            rows: Vec::new(),
        };
        shell.receive(
            0,
            crate::backend::Event::Session {
                generation,
                reply: rows,
            },
            window,
            cx,
        );
        line(
            shell,
            &format!(
                r#"{{"t":"searched","n":0,"scanned":11,"ms":1,"cancelled":false,"gen":{generation},"walking":false}}"#
            ),
            window,
            cx,
        );
    });
    host::frame(&mut cx, window);
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(crate::pane::swap::HOLD_MS));
    cx.run_until_parked();
    host::frame(&mut cx, window);
    let (pane, active, parents) = window
        .update(&mut cx, |shell, window, _| {
            assert_eq!(
                shell.no_match_headline(0).as_deref(),
                Some("NOTHING MATCHES zqq")
            );
            let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
            let parents = columns
                .area
                .tracks()
                .iter()
                .filter(|track| track.slot.index() < Slot::Active.index())
                .count();
            (
                shell.geometry(window).panes[0],
                columns.area.layout.active_x(),
                parents,
            )
        })
        .unwrap();
    assert!(parents > 0, "the work and A columns stay");
    let mut visual = VisualTestContext::from_window(window.into(), &cx);
    let slot = visual
        .debug_bounds("columns-no-match")
        .expect("the headline stands in the columns, not across the pane");
    assert_eq!(f32::from(slot.origin.x), pane.x + active);
    let (top, strip) = window
        .update(&mut cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            let columns = host.columns.as_ref().unwrap();
            let paint = columns.painters.get(Slot::Parent).unwrap().borrow();
            let top = f32::from(paint.drawn.as_ref().unwrap().bounds.origin.y);
            (top, shell.columns_search_strip(0))
        })
        .unwrap();
    assert!(
        strip > 0.,
        "the search strip takes the header slot over Columns"
    );
    assert_eq!(
        top,
        pane.y + strip,
        "the columns start under the search strip"
    );
}

/// The active track's right hairline and the Child track's rows, as the area last laid them out.
fn third(shell: &Shell) -> (Option<f32>, bool) {
    let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
    let tracks = columns.area.tracks();
    let active = tracks.iter().find(|track| track.slot == Slot::Active);
    let child = tracks.iter().find(|track| track.slot == Slot::Child);
    (
        active.and_then(|track| track.divider_x()),
        child.is_some_and(|track| !matches!(track.rows, crate::columns::TrackRows::Blank)),
    )
}

#[test]
fn ey318_no_match_on_a_file_cursor_drops_the_third_column_once_its_hold_ends() {
    let mut cx = host::context();
    let (window, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, window, |shell, window, cx| {
        host::settle(shell, window, cx);
        let host = shell.panes[0].as_mut().unwrap();
        host.pane.held = std::rc::Rc::new(crate::list::pane::Held {
            listing: 3,
            total: 2,
            start: 0,
            rows: vec![host::row("d01", true), host::row("notes.txt", false)],
        });
        host.changed(&shell.locale);
        host.pane.cursor = 1;
        shell.sync_columns(window, cx);
    });
    host::frame(&mut cx, window);
    let rest = window.update(&mut cx, |shell, _, _| third(shell)).unwrap();
    assert!(
        rest.0.is_some(),
        "a file cursor stands a third column at rest"
    );
    // Entering Columns on a file holds its preview until it lands, and the filter answer arrives under that hold.
    host::change(&mut cx, window, |shell, _, cx| {
        let port = &mut shell.panes[0].as_mut().unwrap().preview_port;
        crate::columns::preview_host::ThirdSwap::hold(port, "/srv/a/b/c\n1");
        filter(shell, "zzz", 2, 3, "", cx)
    });
    host::frame(&mut cx, window);
    let held = window.update(&mut cx, |shell, _, _| third(shell)).unwrap();
    assert!(
        held.0.is_some(),
        "the live hold keeps the third column through the answer"
    );
    // The file's preview hold ends by its cap or its landing, which no Columns event follows.
    host::change(&mut cx, window, |shell, _, _| {
        let port = &mut shell.panes[0].as_mut().unwrap().preview_port;
        crate::columns::preview_host::ThirdSwap::cancel(port);
    });
    host::frame(&mut cx, window);
    window
        .update(&mut cx, |shell, _, cx| {
            assert_eq!(footer(shell, cx).0, "0 of 2 shown");
            assert_eq!(
                third(shell),
                (None, false),
                "HIG cols-filter-nomatch: no third column and no hairline right of the active one"
            );
        })
        .unwrap();
}
