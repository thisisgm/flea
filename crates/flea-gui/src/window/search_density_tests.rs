//! Combined current Shell paint after the production density application boundary.
//! The private watched Update constructor is intentionally not bypassed or re-exported.
use super::*;
use crate::window::pane::PaneHost;
use flea::{jsondoc, ui_logic_file_search::Scope};

#[path = "search_density_guards.rs"]
mod guards;
use guards::{open_density, query_identity, start_search, type_query};

fn density(cx: &mut TestAppContext, handle: Handle, word: &str) {
    edit(cx, handle, |shell, window, cx| {
        let patch = jsondoc::parse(&format!(r#"{{"density":"{word}"}}"#)).unwrap();
        let data = flea::uistate::patched(&shell.launch.settings.data, &patch).unwrap();
        for key in [
            "hidden",
            "foldersFirst",
            "hiddenLast",
            "groupByKind",
            "rememberSort",
            "sort",
            "folderSorts",
        ] {
            assert_eq!(
                shell.launch.settings.data.get(key),
                data.get(key),
                "density must not normalize a sparse fixture: {key}"
            );
        }
        shell.apply_settings(data, window, cx);
        assert_eq!(shell.settings.relist, [false, false]);
    });
}

fn results(cx: &mut TestAppContext, handle: Handle, side: usize) {
    let generation = edit(cx, handle, |shell, _, _| {
        shell.panes[side].as_ref().unwrap().search.owner.session.gen as usize
    });
    reply(
        cx,
        handle,
        side,
        &format!(r#"{{"t":"listed","path":"/files","n":128,"gen":{generation}}}"#),
    );
    edit(cx, handle, |shell, window, cx| {
        shell.receive(
            side,
            BackendEvent::Session {
                generation,
                reply: flea::protocol::Reply::Rows {
                    start: 0,
                    listing: 40 + generation,
                    rows: (0..128)
                        .map(|at| RowFacts {
                            name: format!("x-{at:03}.txt"),
                            icon: "text-plain".into(),
                            ..Default::default()
                        })
                        .collect(),
                },
            },
            window,
            cx,
        );
    });
    reply(
        cx,
        handle,
        side,
        &format!(
            r#"{{"t":"searched","n":128,"scanned":128,"ms":1,"cancelled":false,"gen":{generation},"walking":false}}"#
        ),
    );
    frame(cx, handle);
    edit(cx, handle, |shell, _, cx| {
        let host = shell.panes[side].as_mut().unwrap();
        assert_eq!(host.pane.held.total, 128);
        assert!(!host.pane.busy());
        host.selected = vec![1];
        host.pane.cursor = 2;
        host.input.selection.changed(&host.selected);
        cx.notify();
    });
    frame(cx, handle);
}

fn observe(
    cx: &mut TestAppContext,
    handle: Handle,
    side: usize,
    header_frames: u64,
    phase: &str,
) -> (f32, f32) {
    frame(cx, handle);
    edit(cx, handle, |shell, window, _| {
        let host = shell.panes[side].as_ref().unwrap();
        let list = host.list.borrow();
        let strip = host.search.strip.borrow();
        assert!(strip.prepared.is_some(), "SR1_JOIN_STRIP_MOUNT: {phase}");
        assert!(strip.shapings > 0, "the real StripElement shaped: {phase}");
        assert_eq!(
            list.header.drawn.frames, header_frames,
            "ordinary header must not also mount: {phase}"
        );
        assert_eq!(host.search.owner.session.query, "x", "{phase}");
        assert_eq!(host.selected, vec![1], "{phase}");
        assert_eq!(host.pane.cursor, 2, "{phase}");
        assert!(strip.typeface == list.typeface, "{phase}");
        let metrics = list.file_metrics();
        let prepaint = host.input.frame.expect("real input prepaint");
        let painted = host.input.drawn.expect("real input paint").frame;
        assert_eq!(prepaint, painted, "prepaint and paint viewport: {phase}");
        assert_eq!(painted.bounds, list.drawn.bounds, "{phase}");
        assert_eq!(painted.row, metrics.row, "{phase}");
        assert_eq!(list.drawn.metrics.row, metrics.row, "{phase}");
        assert_eq!(list.drawn.metrics.header, metrics.header, "{phase}");
        assert_eq!(
            painted.content,
            128. * metrics.row + metrics.header,
            "{phase}"
        );
        assert_eq!(host.pane.scroll_extent, Some(painted.content), "{phase}");
        assert_eq!(host.pane.scroll_y, painted.scroll, "{phase}");
        let geometry = shell.geometry(window);
        let expected = geometry.rows(
            side,
            shell.view == View::Dual,
            shell.slots.strip_height_for(side),
        );
        assert_eq!(
            f32::from(painted.bounds.top()),
            expected.y,
            "exactly one existing header slot: {phase}"
        );
        for (index, selected, cursor) in [(1, true, false), (2, false, true)] {
            let row = list
                .visible()
                .iter()
                .find(|row| row.index == index)
                .expect("selected/cursor row painted");
            assert!(row.prepared.is_some(), "{phase}");
            assert_eq!(row.state.selected, selected, "{phase}");
            assert_eq!(row.state.cursor, cursor, "{phase}");
        }
        assert!(
            host.search.owner.session.stamps.first_rows_at > 0,
            "actual prepared-row feedback: {phase}"
        );
        println!(
            "SR1_JOIN_SCENE phase={phase} side={side} view={:?} row={} header={} query=x scope={:?}",
            shell.view, metrics.row, metrics.header, host.search.owner.session.scope
        );
        (metrics.row, metrics.header)
    })
}

fn exit(cx: &mut TestAppContext, handle: Handle, side: usize, origin: View) {
    edit(cx, handle, |shell, _, _| shell.focus_side = side);
    key(cx, handle, "escape", None);
    edit(cx, handle, |shell, window, cx| {
        assert!(
            shell.panes[side]
                .as_ref()
                .unwrap()
                .search
                .owner
                .session
                .leaving
        );
        for reply in [
            flea::protocol::Reply::Listed {
                path: "/files".into(),
                total: 1,
            },
            flea::protocol::Reply::Rows {
                start: 0,
                listing: 900,
                rows: vec![RowFacts {
                    name: "restored.txt".into(),
                    ..Default::default()
                }],
            },
        ] {
            shell.receive(
                side,
                BackendEvent::Session {
                    generation: 0,
                    reply,
                },
                window,
                cx,
            );
        }
    });
    frame(cx, handle);
    // Let the real mounted swap and presentation deadlines retire before checking the landing.
    cx.executor()
        .advance_clock(Duration::from_millis(crate::pane::swap::HOLD_MS));
    cx.run_until_parked();
    frame(cx, handle);
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[side].as_ref().unwrap();
        assert!(
            !host.pane.busy(),
            "ordinary relist must actually land after its deadline"
        );
        assert!(
            !host.pane.state.search,
            "the restored rows must be an ordinary listing"
        );
        assert!(host.pane.swap.pending.is_none());
        assert_eq!(host.pane.held.listing, 900);
        assert_eq!(host.pane.held.total, 1);
        assert_eq!(host.pane.held.row(0).unwrap().name, "restored.txt");
        assert!(!shell.panes[side].as_ref().unwrap().search.owner.active());
        assert_eq!(
            shell.view, origin,
            "Escape restores originating view after real relist landing"
        );
    });
    println!("SR1_JOIN_SCENE phase=escape-landed side={side} origin={origin:?}");
}

#[test]
fn joined_folder_home_list_density_draw() {
    for scope in [Scope::Folder, Scope::Home] {
        let mut cx = crate::list::bench::real_cx();
        let (handle, _read) = open_density(&mut cx);
        density(&mut cx, handle, "compact");
        frame(&mut cx, handle);
        let header = edit(&mut cx, handle, |shell, _, _| {
            // Columns searches in its own active column, so the List density runs from the List.
            shell.view = View::List;
            shell.panes[0]
                .as_ref()
                .unwrap()
                .list
                .borrow()
                .header
                .drawn
                .frames
        });
        start_search(&mut cx, handle, 0, View::List);
        let generation = type_query(&mut cx, handle, 0, scope);
        results(&mut cx, handle, 0);
        edit(&mut cx, handle, |shell, _, _| {
            assert_eq!(
                shell.panes[0].as_ref().unwrap().search.owner.session.scope,
                scope
            )
        });
        let before = observe(&mut cx, handle, 0, header, "list-query-results-compact");
        let identity = query_identity(&mut cx, handle, 0);
        assert_eq!(identity.0, generation);
        density(&mut cx, handle, "comfortable");
        assert_eq!(query_identity(&mut cx, handle, 0), identity);
        let after = observe(
            &mut cx,
            handle,
            0,
            header,
            "list-density-applied-query-results",
        );
        assert_ne!(before.0, after.0);
        exit(&mut cx, handle, 0, View::List);
    }
}

#[test]
fn joined_dual_independent_search_density_draw() {
    let mut cx = crate::list::bench::real_cx();
    let (handle, _read) = open_density(&mut cx);
    let (right_backend, _right_read) = Backend::test_lines();
    edit(&mut cx, handle, |shell, _, cx| {
        let mut right = PaneHost::new("/files".into(), None, 620., &shell.locale);
        right.pane.use_executor(cx.background_executor().clone());
        right.backend = Some(right_backend);
        right
            .pane
            .receive(flea::protocol::Reply::Listed {
                path: "/files".into(),
                total: 0,
            })
            .unwrap();
        right
            .pane
            .receive(flea::protocol::Reply::Rows {
                start: 0,
                listing: 3,
                rows: vec![],
            })
            .unwrap();
        shell.panes[1] = Some(right);
        shell.lifecycle.attach(1);
        shell.view = View::Dual;
    });
    density(&mut cx, handle, "compact");
    frame(&mut cx, handle);
    let mut headers = [0; 2];
    for (side, header) in headers.iter_mut().enumerate() {
        edit(&mut cx, handle, |shell, _, _| {
            *header = shell.panes[side]
                .as_ref()
                .unwrap()
                .list
                .borrow()
                .header
                .drawn
                .frames;
        });
        start_search(&mut cx, handle, side, View::Dual);
        type_query(&mut cx, handle, side, Scope::Home);
        results(&mut cx, handle, side);
    }
    for (side, header) in headers.into_iter().enumerate() {
        observe(
            &mut cx,
            handle,
            side,
            header,
            "dual-both-query-results-compact",
        );
    }
    let identities = [
        query_identity(&mut cx, handle, 0),
        query_identity(&mut cx, handle, 1),
    ];
    density(&mut cx, handle, "comfortable");
    for side in 0..2 {
        assert_eq!(query_identity(&mut cx, handle, side), identities[side]);
        observe(
            &mut cx,
            handle,
            side,
            headers[side],
            "dual-density-applied-both-query-results",
        );
    }
    exit(&mut cx, handle, 1, View::Dual);
    edit(&mut cx, handle, |shell, _, _| {
        assert_eq!(
            shell.panes[0].as_ref().unwrap().search.owner.session.query,
            "x"
        )
    });
    exit(&mut cx, handle, 0, View::Dual);
}
