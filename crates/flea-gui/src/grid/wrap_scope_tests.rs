//! Optional row crossing stays within the focused pane and its visible order.
use super::host::GridHost;
use super::join_tests::{frame, key, open};
use super::wrap_tests::configured;
use crate::{grid::Shown, window::pane::PaneHost, window::startup::View};
use gpui::TestAppContext;
use std::rc::Rc;

// Hidden entries never reach held rows from the backend, so this pins crossing to the order the listing sent.
#[gpui::test]
fn issue_434_row_crossing_follows_the_listing_order_the_backend_sent(cx: &mut TestAppContext) {
    for hidden in [false, true] {
        let (handle, _requests) = open(cx);
        handle
            .update(cx, |shell, window, cx| {
                shell.launch.settings = crate::window::startup::Settings::from_json(
                    flea::jsondoc::parse(&format!(r#"{{"gridWrapRows":true,"hidden":{hidden}}}"#))
                        .unwrap(),
                );
                let mut names: Vec<String> = (0..24).map(|i| format!("file-{i:02}.txt")).collect();
                if hidden {
                    names.insert(0, ".hidden.txt".into());
                }
                let host = shell.panes[0].as_mut().unwrap();
                host.pane.held = Rc::new(crate::pane::Held {
                    listing: 2,
                    total: names.len(),
                    start: 0,
                    rows: names
                        .into_iter()
                        .map(|name| flea::protocol::RowFacts {
                            name,
                            ..Default::default()
                        })
                        .collect(),
                });
                shell.set_view(View::Grid, window, cx);
            })
            .unwrap();
        frame(cx, handle);
        handle
            .update(cx, |shell, _, cx| {
                let host = shell.panes[0].as_mut().unwrap();
                let columns = host
                    .grid
                    .as_ref()
                    .unwrap()
                    .view
                    .layout(Shown::All(host.pane.held.total))
                    .columns;
                assert!(columns > 1 && columns < 24);
                host.pane.cursor = columns - 1;
                assert!(shell.grid_key("cursorRight", cx));
                let host = shell.panes[0].as_ref().unwrap();
                assert_eq!(host.pane.cursor, columns);
                assert_eq!(
                    host.pane.held.row(columns).unwrap().name,
                    format!("file-{:02}.txt", columns - usize::from(hidden))
                );
                assert!(shell.grid_key("cursorLeft", cx));
                assert_eq!(shell.panes[0].as_ref().unwrap().pane.cursor, columns - 1);
            })
            .unwrap();
    }
}

#[gpui::test]
fn issue_434_each_pane_uses_its_own_columns_and_cursor(cx: &mut TestAppContext) {
    let (handle, _requests) = configured(cx, r#"{"gridWrapRows":true}"#);
    handle
        .update(cx, |shell, _, cx| {
            let mut right = PaneHost::new("/right".into(), None, 620., &shell.locale);
            right.pane.held = Rc::clone(&shell.panes[0].as_ref().unwrap().pane.held);
            right.settle_grid((300., 400.), &shell.launch.settings.data, &shell.locale);
            shell.panes[1] = Some(right);
            for side in 0..2 {
                shell.focus_side = side;
                let host = shell.panes[side].as_mut().unwrap();
                let columns = host
                    .grid
                    .as_ref()
                    .unwrap()
                    .view
                    .layout(Shown::All(host.pane.held.total))
                    .columns;
                host.pane.cursor = columns - 1;
                let other = shell.panes[1 - side].as_ref().unwrap().pane.cursor;
                assert!(shell.grid_key("cursorRight", cx));
                assert_eq!(shell.panes[side].as_ref().unwrap().pane.cursor, columns);
                assert_eq!(shell.panes[1 - side].as_ref().unwrap().pane.cursor, other);
                assert!(shell.grid_key("cursorLeft", cx));
                assert_eq!(shell.panes[side].as_ref().unwrap().pane.cursor, columns - 1);
            }
            shell.view = View::Dual;
            assert!(
                !shell.grid_key("cursorRight", cx),
                "Dual keeps its existing List routing"
            );
        })
        .unwrap();
}

#[gpui::test]
fn issue_434_filtered_wrap_extends_only_visible_rows_and_reveals_the_target(
    cx: &mut TestAppContext,
) {
    let (handle, _requests) = configured(cx, r#"{"gridWrapRows":true}"#);
    let (columns, shown, at) = handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_mut().unwrap();
            let columns = host
                .grid
                .as_ref()
                .unwrap()
                .view
                .layout(Shown::All(1000))
                .columns;
            let shown: Rc<[usize]> = (0..100).map(|i| i * 3 + 1).collect::<Vec<_>>().into();
            let at = columns * 8 - 1;
            host.filter.shown = Some(Rc::clone(&shown));
            host.input.set_shown(Some(Rc::clone(&shown)));
            host.pane.cursor = shown[at];
            // The backend's sorted order assigns listing positions; hidden entries never enter it.
            let mut rows = host.pane.held.rows.clone();
            for (index, row) in rows.iter_mut().enumerate() {
                row.name = format!("file-{:03}.txt", 999 - index);
            }
            host.pane.held = Rc::new(crate::pane::Held {
                listing: host.pane.held.listing,
                total: host.pane.held.total,
                start: host.pane.held.start,
                rows,
            });
            (columns, shown, at)
        })
        .unwrap();
    key(cx, handle, "shift-right");
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            let grid = host.grid.as_ref().unwrap();
            assert_eq!(host.pane.cursor, shown[at + 1]);
            assert_eq!(host.selected, vec![shown[at], shown[at + 1]]);
            assert_eq!(grid.state.borrow().layout.unwrap().columns, columns);
            assert!(grid.view.scroll > 0.);
            let layout = grid.view.layout(Shown::Only(&shown));
            assert!(layout.covers(grid.view.scroll, at + 1));
        })
        .unwrap();
    key(cx, handle, "shift-left");
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.pane.cursor, shown[at]);
            assert_eq!(host.selected, vec![shown[at]]);
        })
        .unwrap();
}

#[gpui::test]
fn filtered_grid_scroll_asks_the_rows_its_viewport_draws(cx: &mut TestAppContext) {
    let (handle, mut requests) = configured(cx, "{}");
    let shown: Rc<[usize]> = (0..1000)
        .filter(|row| row % 10 != 0)
        .collect::<Vec<_>>()
        .into();
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_mut().unwrap();
            host.filter.shown = Some(Rc::clone(&shown));
            host.input.set_shown(Some(Rc::clone(&shown)));
            host.pane.cursor = shown[0];
        })
        .unwrap();
    frame(cx, handle);
    while requests().is_some() {}
    let drawn = handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_mut().unwrap();
            let focus = GridHost::focus(&host.pane);
            let grid = host.grid.as_mut().unwrap();
            let layout = grid.view.layout(Shown::Only(&shown));
            let deep = layout.row_top(700 / layout.columns);
            grid.view.scroll_by(Shown::Only(&shown), &focus, deep);
            let first = layout.viewport(grid.view.scroll).0.max(0);
            shown[usize::try_from(first).unwrap()]
        })
        .unwrap();
    assert!(
        drawn >= 320,
        "the scrolled tiles lie past the fixture's held rows"
    );
    frame(cx, handle);
    let (start, count) =
        requests().expect("a filtered scroll past the held rows asks their window");
    assert!(
        start <= drawn && drawn < start + count,
        "{start}+{count} misses row {drawn}"
    );
}

#[gpui::test]
fn filtered_arrow_from_a_hidden_cursor_lands_on_the_filter_like_the_list(cx: &mut TestAppContext) {
    let (handle, _requests) = configured(cx, "{}");
    let shown: Rc<[usize]> = (0..100).map(|i| i * 3 + 1).collect::<Vec<_>>().into();
    handle
        .update(cx, |shell, _, cx| {
            for (hidden, action) in [(5, "cursorLeft"), (5, "cursorRight"), (500, "cursorRight")] {
                let host = shell.panes[0].as_mut().unwrap();
                host.filter.shown = Some(Rc::clone(&shown));
                host.input.set_shown(Some(Rc::clone(&shown)));
                host.pane.cursor = hidden;
                assert!(shell.grid_key(action, cx));
                let cursor = shell.panes[0].as_ref().unwrap().pane.cursor;
                let (step, _) =
                    flea::ui_logic_listing_filter::step(Some(&shown), 1000, hidden, 1).unwrap();
                assert_eq!(
                    cursor, step,
                    "{action} from hidden row {hidden} lands where the filter's Down does"
                );
                assert_eq!(cursor, shown[0]);
            }
        })
        .unwrap();
}
