//! A standing filter in every view: the status count (EY-09), Columns' active rows (EY-10) and the no-match headline (EY-11, EY-12).
use crate::{
    backend::Backend,
    state::{tests::shell_with, Shell},
    window::{root::Root, startup::View},
};
use flea::ui_logic_file_search::Mode;
use gpui::{AppContext, Context, TestAppContext, VisualTestContext, Window, WindowHandle};

#[path = "filter_views_columns_tests.rs"]
mod columns;

#[path = "filter_exit_columns_tests.rs"]
mod columns_exit;

#[path = "fixreaders_filter_tests.rs"]
mod fixreaders;

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

#[gpui::test]
fn ey390_grid_captions_mark_the_filter_run(cx: &mut TestAppContext) {
    let handle = open(cx);
    edit(cx, handle, |shell, _, _| {
        let pane = &mut shell.panes[0].as_mut().unwrap().pane;
        pane.held = std::rc::Rc::new(crate::pane::Held {
            listing: 2,
            total: 1000,
            start: 0,
            rows: (0..320)
                .map(|row| flea::protocol::RowFacts {
                    name: format!("image-{row:03}.png"),
                    ..Default::default()
                })
                .collect(),
        });
        shell.view = View::Grid;
    });
    frame(cx, handle);
    edit(cx, handle, |shell, _, cx| {
        filter(shell, "image", 1000, 2, "3,5,7", cx)
    });
    frame(cx, handle);
    edit(cx, handle, |shell, _, _| {
        let grid = shell.panes[0].as_ref().unwrap().grid.as_ref().unwrap();
        let state = grid.state.borrow();
        let marked: Vec<_> = state
            .drawn()
            .iter()
            .filter(|tile| {
                tile.caption_text()
                    .is_some_and(|caption| caption.contains("image"))
            })
            .map(|tile| (tile.cursor, tile.washes[0].is_some()))
            .collect();
        assert!(!marked.is_empty(), "the filter's rows draw tiles");
        assert!(
            marked.iter().all(|&(cursor, washed)| cursor || washed),
            "HIG grid-filter-matches.png and v0.3.12 wash the typed run in every other tile: {marked:?}"
        );
    });
}

/// EY-470: stateLayers reports the no-match headline the pane draws, as message true with its text.
#[gpui::test]
fn ey470_state_layers_reads_the_drawn_filter_no_match_headline(cx: &mut TestAppContext) {
    let handle = open(cx);
    frame(cx, handle);
    edit(cx, handle, |shell, _, cx| {
        filter(shell, "zzz", 1000, 2, "", cx)
    });
    frame(cx, handle);
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    assert!(
        visual.debug_bounds("no-match-hero").is_some(),
        "the headline is drawn"
    );
    let reply = edit(cx, handle, |shell, window, cx| {
        let (answer, receive) = std::sync::mpsc::sync_channel(1);
        shell.ipc(
            crate::ipc::server::Event::Call {
                request: flea::ipc::wire::Request::Call {
                    target: "flea".into(),
                    function: "stateLayers".into(),
                    arguments: vec![],
                },
                answer,
            },
            window,
            cx,
        );
        receive.try_recv().unwrap().reply
    });
    assert_eq!(reply.code, 0, "{}", reply.stderr);
    let layers = flea::jsondoc::parse(reply.stdout.trim()).unwrap();
    assert_eq!(layers.get("message").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(
        layers.get("headline").and_then(|v| v.as_str()),
        Some("NOTHING MATCHES zzz")
    );
}
