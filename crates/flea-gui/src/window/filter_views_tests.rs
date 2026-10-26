//! A standing filter in every view: the status count (EY-09), Columns' active rows (EY-10) and the no-match headline (EY-11, EY-12).
use crate::{
    backend::Backend,
    columns::geometry::Slot,
    state::{tests::shell_with, Shell},
    window::{columns_tests::host, root::Root, startup::View},
};
use flea::ui_logic_sr1::Mode;
use gpui::{AppContext, Context, TestAppContext, VisualTestContext, Window, WindowHandle};

type Handle = WindowHandle<Root>;

fn open(cx: &mut TestAppContext) -> Handle {
    let (backend, _read) = Backend::test_lines();
    cx.add_window(|window, cx| Root {
        shell: cx.new(|cx| {
            let shell = shell_with(backend, cx);
            window.focus(&shell.focus, cx);
            shell
        }),
    })
}

fn edit<R>(
    cx: &mut TestAppContext,
    handle: Handle,
    f: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>) -> R,
) -> R {
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| f(shell, window, cx))
        })
        .unwrap()
}

fn frame(cx: &mut TestAppContext, handle: impl Into<gpui::AnyWindowHandle>) {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
    cx.run_until_parked();
}

/// Types `query` on a fresh filter line and lands `rows` as its answer for `listing` of `n`.
fn filter(
    shell: &mut Shell,
    query: &str,
    n: usize,
    listing: usize,
    rows: &str,
    cx: &mut Context<Shell>,
) {
    shell.filter_update(
        0,
        |filter, pane| {
            filter.close(pane);
            filter.start();
            None
        },
        cx,
    );
    shell.filter_update(0, |filter, pane| filter.typed(query, pane), cx);
    let id = shell.panes[0]
        .as_ref()
        .unwrap()
        .filter
        .owner
        .asked()
        .expect("the key asked")
        .id;
    let line = format!(
        r#"{{"t":"filtered","id":{id},"query":"{query}","n":{n},"rows":[{rows}],"ms":0.010,"listing":{listing}}}"#
    );
    assert!(shell.filter_line(0, &line, cx));
}

fn footer(shell: &Shell, cx: &gpui::App) -> (String, String) {
    let footer = shell.surfaces[0]
        .footer
        .as_ref()
        .expect("the footer mounted")
        .read(cx);
    let lane = footer.line.read(cx).drawn().primary.to_string();
    (footer.counts.to_string(), lane)
}

#[gpui::test]
fn ey09_status_counts_the_filter_in_list_and_grid(cx: &mut TestAppContext) {
    let handle = open(cx);
    frame(cx, handle);
    edit(cx, handle, |shell, _, cx| {
        filter(shell, "n", 1000, 2, "3,5,700", cx)
    });
    frame(cx, handle);
    assert_eq!(
        edit(cx, handle, |shell, _, cx| footer(shell, cx)).0,
        "3 of 1,000 shown",
        "HIG filter.png prints the shown count"
    );
    edit(cx, handle, |shell, _, _| shell.view = View::Grid);
    frame(cx, handle);
    assert_eq!(
        edit(cx, handle, |shell, _, cx| footer(shell, cx)).0,
        "3 of 1,000 shown",
        "HIG grid-filter-matches.png too"
    );
    edit(cx, handle, |shell, _, cx| {
        shell.filter_update(
            0,
            |filter, pane| {
                filter.escape(pane);
                filter.escape(pane);
                None
            },
            cx,
        )
    });
    frame(cx, handle);
    assert_eq!(
        edit(cx, handle, |shell, _, cx| footer(shell, cx)).0,
        "1,000 items",
        "a cleared filter counts the folder again"
    );
}

#[gpui::test]
fn ey09_search_receipt_stands_before_the_disk_facts(cx: &mut TestAppContext) {
    let handle = open(cx);
    frame(cx, handle);
    edit(cx, handle, |shell, _, _| {
        let session = &mut shell.panes[0].as_mut().unwrap().search.owner.session;
        (
            session.mode,
            session.query,
            session.total,
            session.running,
            session.ms,
        ) = (Mode::Results, "not".into(), 2, false, 12.);
    });
    frame(cx, handle);
    assert_eq!(
        edit(cx, handle, |shell, _, cx| footer(shell, cx)).1,
        "2 found in 0.0 s",
        "HIG search.png"
    );
    edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_mut().unwrap().search.owner.session.total = 0
    });
    frame(cx, handle);
    assert_eq!(
        edit(cx, handle, |shell, _, cx| footer(shell, cx)).1,
        "",
        "CHANGES ruling 12: no 0 found in 0.0 s"
    );
}

#[gpui::test]
fn ey11_a_filter_that_matches_nothing_draws_the_headline_in_list_and_grid(cx: &mut TestAppContext) {
    let handle = open(cx);
    frame(cx, handle);
    edit(cx, handle, |shell, _, cx| {
        filter(shell, "zzz", 1000, 2, "", cx)
    });
    for view in [View::List, View::Grid] {
        edit(cx, handle, |shell, _, _| shell.view = view);
        frame(cx, handle);
        assert_eq!(
            edit(cx, handle, |shell, _, _| shell.no_match_headline(0)).as_deref(),
            Some("NOTHING MATCHES zzz")
        );
        let mut visual = VisualTestContext::from_window(handle.into(), cx);
        assert!(
            visual.debug_bounds("no-match-hero").is_some(),
            "{view:?} draws the search mark and its sentence"
        );
        let drawn = edit(cx, handle, |shell, _, _| {
            shell.panes[0].as_ref().unwrap().drawn_view
        });
        let expected = (view == View::List).then_some(View::List);
        assert_eq!(
            drawn, expected,
            "only a List hero claims the List's pointer readers"
        );
    }
    edit(cx, handle, |shell, _, cx| {
        filter(shell, "z", 1000, 2, "4", cx)
    });
    assert_eq!(
        edit(cx, handle, |shell, _, _| shell.no_match_headline(0)),
        None,
        "a match takes the headline away"
    );
}

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
