//! Ranked close matches retain their view coordinates through movement, selection and reveal.
use super::*;
use crate::{grid::Shown, input::selection::Selection};
use gpui::{point, px, size, Bounds, MouseButton, MouseDownEvent, MouseUpEvent};
use std::rc::Rc;

const RANKED: [usize; 4] = [2880, 60, 1500, 300];

fn ranked(cx: &mut TestAppContext, view: View, rows: &[usize]) -> Handle {
    let (handle, _read) = open(cx);
    edit(cx, handle, |shell, _, _| {
        shell.view = view;
        shell.panes[0].as_mut().unwrap().pane.held = Rc::new(crate::pane::Held {
            listing: 2,
            total: 3000,
            start: 0,
            rows: (0..3000)
                .map(|at| flea::protocol::RowFacts {
                    name: format!("file-{at}.txt"),
                    ..Default::default()
                })
                .collect(),
        });
    });
    frame(cx, handle);
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.filter.shown = Some(Rc::from(rows));
        host.input.set_shown(Some(Rc::from(rows)));
        let frame = host.input.frame.as_mut().unwrap();
        frame.total = rows.len();
        frame.columns = 1;
        frame.row = 20.;
        frame.content = rows.len() as f32 * 20.;
        frame.scroll = 0.;
        frame.bounds = Bounds::new(point(px(0.), px(0.)), size(px(200.), px(80.)));
        frame.cell_origin = (0., 0.);
        frame.cell_width = 180.;
        host.input.hit_boxes = None;
        if let Some(grid) = host.grid.as_mut() {
            grid.view.size = (160., 140.);
            grid.view.scroll = 0.;
            assert_eq!(grid.view.layout(Shown::Only(rows)).columns, 1);
        }
    });
    handle
}

fn moves(cx: &mut TestAppContext, view: View, rows: &[usize]) {
    let handle = ranked(cx, view, rows);
    edit(cx, handle, |shell, window, cx| {
        for (at, &row) in rows.iter().enumerate() {
            for (key, next) in [("down", (at + 1).min(3)), ("up", at.saturating_sub(1))] {
                shell.panes[0].as_mut().unwrap().pane.cursor = row;
                shell.key(
                    &KeyDownEvent {
                        keystroke: Keystroke::parse(key).unwrap(),
                        is_held: false,
                        prefer_character_input: false,
                    },
                    window,
                    cx,
                );
                assert_eq!(
                    shell.panes[0].as_ref().unwrap().pane.cursor,
                    rows[next],
                    "{view:?} {key} from view {at} (listing {row}) follows drawn order"
                );
            }
        }
    });
}

#[gpui::test]
fn ranked_filter_list_down_and_up_visit_every_close_match(cx: &mut TestAppContext) {
    moves(cx, View::List, &RANKED);
}

#[gpui::test]
fn ranked_filter_grid_down_and_up_keep_every_cursor_view(cx: &mut TestAppContext) {
    moves(cx, View::Grid, &RANKED);
}

#[gpui::test]
fn ranked_filter_exact_order_keeps_list_and_grid_steps(cx: &mut TestAppContext) {
    for view in [View::List, View::Grid] {
        moves(cx, view, &[60, 300, 1500, 2880]);
    }
}

#[gpui::test]
fn ranked_filter_pointer_chooses_the_drawn_match_then_reveals_its_neighbour(
    cx: &mut TestAppContext,
) {
    for view in [View::List, View::Grid] {
        let handle = ranked(cx, view, &RANKED);
        edit(cx, handle, |shell, window, cx| {
            for (at, &row) in RANKED.iter().enumerate() {
                let position = point(px(50.), px(20. * (at as f32 + 0.5)));
                shell.pointer_down(
                    0,
                    &MouseDownEvent {
                        button: MouseButton::Left,
                        position,
                        click_count: 1,
                        ..Default::default()
                    },
                    window,
                    cx,
                );
                shell.pointer_up(
                    0,
                    &MouseUpEvent {
                        button: MouseButton::Left,
                        position,
                        click_count: 1,
                        ..Default::default()
                    },
                    window,
                    cx,
                );
                assert_eq!(shell.panes[0].as_ref().unwrap().pane.cursor, row);
                shell.key(
                    &KeyDownEvent {
                        keystroke: Keystroke::parse("up").unwrap(),
                        is_held: false,
                        prefer_character_input: false,
                    },
                    window,
                    cx,
                );
                assert_eq!(
                    shell.panes[0].as_ref().unwrap().pane.cursor,
                    RANKED[at.saturating_sub(1)]
                );
            }
        });
    }
}

#[gpui::test]
fn ranked_filter_rename_reveal_uses_the_edited_view_row(cx: &mut TestAppContext) {
    let handle = ranked(cx, View::Grid, &RANKED);
    edit(cx, handle, |shell, window, cx| {
        shell.panes[0].as_mut().unwrap().pane.cursor = 300;
        assert!(shell.rename_open(0, 1, window, cx));
        assert_eq!(shell.renaming_row(0), Some(300));
        shell.bands[0] = 30.;
        let grid = shell.panes[0].as_ref().unwrap().grid.as_ref().unwrap();
        let expected = grid.view.layout(Shown::Only(&RANKED)).reveal(0., 3);
        assert!(expected > 0.);
        shell.reveal_rename_grid([0.; 2]);
        assert_eq!(
            shell.panes[0]
                .as_ref()
                .unwrap()
                .grid
                .as_ref()
                .unwrap()
                .view
                .scroll,
            expected
        );
    });
}

#[gpui::test]
fn ranked_filter_painted_grid_keeps_cursor_and_selected_tiles(cx: &mut TestAppContext) {
    let handle = ranked(cx, View::Grid, &[60, 300, 1500, 2880]);
    press(cx, handle, "/", Some("/"), false);
    press(cx, handle, "f", Some("f"), false);
    reply(
        cx,
        handle,
        r#"{"t":"filtered","id":1,"query":"f","n":3000,"rows":[],"close":[60,1500,300,2880],"ms":0.01,"listing":2}"#,
    );
    for row in [60, 1500, 300, 2880] {
        edit(cx, handle, |shell, _, _| {
            let host = shell.panes[0].as_mut().unwrap();
            host.pane.cursor = row;
            host.selected = vec![60, 300, 1500, 2880];
        });
        frame(cx, handle);
        edit(cx, handle, |shell, _, _| {
            let state = shell.panes[0]
                .as_ref()
                .unwrap()
                .grid
                .as_ref()
                .unwrap()
                .state
                .borrow();
            assert_eq!(
                state
                    .drawn()
                    .iter()
                    .filter(|tile| tile.cursor)
                    .map(|tile| tile.listing)
                    .collect::<Vec<_>>(),
                [row]
            );
            for tile in state.drawn() {
                assert!(
                    tile.selected,
                    "ranked tile {} keeps its selected state",
                    tile.listing
                );
            }
        });
    }
}

#[gpui::test]
fn ranked_filter_kinetic_pull_clamps_in_view_coordinates(cx: &mut TestAppContext) {
    let handle = ranked(cx, View::List, &RANKED);
    edit(cx, handle, |shell, _, _| {
        let input = &mut shell.panes[0].as_mut().unwrap().input;
        input.frame.as_mut().unwrap().bounds.size.height = px(40.);
        let mut cursor = 300;
        input.scrolled(20., &mut cursor);
        assert_eq!(
            cursor, 1500,
            "the last whole view row is 2, not listing row 300"
        );
    });
}

#[test]
fn ranked_filter_grid_span_covers_every_listing_row_in_the_view_range() {
    let shown = Shown::Only(&RANKED);
    assert_eq!(shown.span(0, 3), Some(60..=2880));
    assert_eq!(shown.span(1, 3), Some(60..=1500));
    assert_eq!(shown.span(2, 2), Some(1500..=1500));
}

fn owner(rows: &[usize]) -> crate::input::PaneInput {
    let mut input = crate::input::PaneInput::default();
    input.set_shown(Some(Rc::from(rows)));
    input
}

#[test]
fn ranked_filter_shift_range_follows_positions_and_shrinks() {
    let input = owner(&RANKED);
    let mut selection = Selection::default();
    let mut selected = Vec::new();
    selection.extend(&mut selected, 2880, 1500, None, &input.shown);
    assert_eq!(selected, [60, 1500, 2880]);
    selection.extend(&mut selected, 1500, 60, None, &input.shown);
    assert_eq!(selected, [60, 2880]);
}

#[test]
fn ranked_filter_hidden_anchor_still_selects_its_range() {
    let exact = owner(&[10, 60, 100]);
    let mut selection = Selection::default();
    let mut selected = Vec::new();
    selection.extend(&mut selected, 50, 100, None, &exact.shown);
    assert_eq!(
        selected,
        [60, 100],
        "an exact answer keeps the listing range"
    );
    let ranked = owner(&RANKED);
    let mut selection = Selection::default();
    let mut selected = Vec::new();
    selection.extend(&mut selected, 5, 60, None, &ranked.shown);
    assert_eq!(selected, [60, 2880], "a hidden anchor stands before view 0");
}

#[test]
fn ranked_filter_exact_answer_keeps_insertion_points_for_hidden_rows() {
    let exact = owner(&[10, 60, 100]);
    for (row, view) in [(5, 0), (10, 0), (50, 1), (60, 1), (100, 2), (150, 3)] {
        assert_eq!(exact.view_index(row), view, "listing row {row}");
    }
    let ranked = owner(&RANKED);
    assert_eq!((ranked.view_index(5), ranked.view_index(300)), (0, 3));
    assert_eq!(crate::input::PaneInput::default().view_index(42), 42);
}

#[gpui::test]
fn ranked_filter_pointer_press_reveals_the_pressed_view_row(cx: &mut TestAppContext) {
    let handle = ranked(cx, View::List, &RANKED);
    edit(cx, handle, |shell, window, cx| {
        // View 3 is cut off at scroll 30, so only its own index reveals it at 40; view 0 or 2 would not.
        for (scroll, y, row, revealed) in [(0., 10., 2880, 0.), (30., 35., 300, 40.)] {
            let host = shell.panes[0].as_mut().unwrap();
            let frame = host.input.frame.as_mut().unwrap();
            frame.bounds.size.height = px(40.);
            frame.scroll = scroll;
            host.pane.scroll_y = scroll;
            let position = point(px(50.), px(y));
            let down = MouseDownEvent {
                button: MouseButton::Left,
                position,
                click_count: 1,
                ..Default::default()
            };
            shell.pointer_down(0, &down, window, cx);
            let up = MouseUpEvent {
                button: MouseButton::Left,
                position,
                click_count: 1,
                ..Default::default()
            };
            shell.pointer_up(0, &up, window, cx);
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.pane.cursor, row);
            assert_eq!(
                host.pane.scroll_y, revealed,
                "listing {row} reveals its own view row"
            );
        }
    });
}

#[test]
fn ranked_filter_select_all_preserves_sorted_selection_for_toggle_and_invert() {
    let mut selection = Selection::default();
    let mut selected = Vec::new();
    selection.all(&mut selected, 3000, Some(&RANKED), false);
    assert_eq!(selected, [60, 300, 1500, 2880]);
    selection.toggle(&mut selected, 2880);
    assert_eq!(selected, [60, 300, 1500]);
    selection.all(&mut selected, 3000, Some(&RANKED), true);
    assert_eq!(selected, [2880]);
}

#[path = "filter_ranked_timing_tests.rs"]
mod timing;
