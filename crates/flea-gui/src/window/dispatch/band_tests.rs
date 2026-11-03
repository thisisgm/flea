//! Selection bands enter through the mounted view and keep advancing with a stationary pointer.
use super::test_shell::{land, open_root_with, row};
use crate::{columns::geometry::Slot, state::Shell};
use gpui::{
    point, px, size, Entity, Modifiers, MouseButton, Pixels, Point, ScrollDelta, ScrollWheelEvent,
    TestAppContext, VisualTestContext,
};
use std::time::Duration;

fn open<'a>(
    cx: &'a mut TestAppContext,
    view: &str,
    count: usize,
) -> (Entity<Shell>, &'a mut VisualTestContext) {
    let (shell, cx) = open_root_with(cx, &format!(r#""keys":"default","view":"{view}""#), None);
    land(
        &shell,
        cx,
        "/files",
        (0..count)
            .map(|i| row(&format!("item-{i:02}.txt"), false))
            .collect(),
        1,
    );
    (shell, cx)
}

fn drag(cx: &mut VisualTestContext, start: Point<Pixels>, end: Point<Pixels>) {
    cx.simulate_mouse_move(start, None, Modifiers::none());
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::none());
    cx.run_until_parked();
}

fn cancelled_band(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    start: Point<Pixels>,
    end: Point<Pixels>,
) {
    cx.simulate_keystrokes("escape v");
    let before = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().selected.clone()
    });
    assert_eq!(before, [0]);
    let control = Modifiers {
        control: true,
        ..Default::default()
    };
    cx.simulate_mouse_down(start, MouseButton::Left, control);
    cx.simulate_mouse_move(end, Some(MouseButton::Left), control);
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| {
        assert!(shell.panes[0].as_ref().unwrap().selected.len() > 1)
    });
    cx.simulate_keystrokes("escape");
    cx.simulate_mouse_up(end, MouseButton::Left, control);
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.selected, before, "Escape restores the original marks");
        assert!(host.input.band.is_none());
    });
    drag(cx, start, end);
    cx.simulate_resize(size(px(1000.), px(680.)));
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(
            host.selected, before,
            "resize restores marks before repainting"
        );
        assert!(host.input.band.is_none());
    });
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
}

#[gpui::test]
fn fix_band_ey125_columns_empty_ground_selects_and_paints_rows(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "columns", 4);
    let (start, end) = shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        let paint = host
            .columns
            .as_ref()
            .unwrap()
            .painters
            .get(Slot::Active)
            .unwrap()
            .borrow();
        let drawn = paint.drawn.as_ref().expect("active column is mounted");
        (
            point(
                drawn.bounds.left() + px(100.),
                drawn.bounds.bottom() - px(40.),
            ),
            point(drawn.bounds.left() + px(30.), drawn.bounds.top() + px(10.)),
        )
    });
    drag(cx, start, end);
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(
            host.selected,
            [0, 1, 2, 3],
            "empty column ground starts the shared band"
        );
        assert!(host.input.band.as_ref().is_some_and(|band| band.active));
        assert!(host.input.drawn.is_some(), "band overlay is mounted");
    });
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.pane.cursor, 0);
        assert_eq!(host.selected, [0, 1, 2, 3]);
        assert!(host.input.band.is_none());
    });
    cancelled_band(&shell, cx, start, end);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn fix_band_ey126_grid_empty_ground_selects_and_paints_tiles(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "grid", 10);
    let (start, end) = shell.read_with(cx, |shell, _| {
        let grid = shell.panes[0].as_ref().unwrap().grid.as_ref().unwrap();
        let state = grid.state.borrow();
        let tile = state.drawn().first().expect("grid tiles are mounted");
        (
            point(
                state.bounds.right() - px(40.),
                state.bounds.bottom() - px(40.),
            ),
            point(
                state.bounds.left() + px(20.),
                state.bounds.top() + px(tile.cell.y + 10.),
            ),
        )
    });
    drag(cx, start, end);
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(
            host.selected,
            (0..10).collect::<Vec<_>>(),
            "empty grid ground starts the shared band"
        );
        assert!(host.input.band.as_ref().is_some_and(|band| band.active));
        assert!(host
            .grid
            .as_ref()
            .unwrap()
            .state
            .borrow()
            .drawn()
            .iter()
            .all(|tile| tile.selected));
    });
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.pane.cursor, 0);
        assert_eq!(host.selected, (0..10).collect::<Vec<_>>());
        assert!(host.input.band.is_none());
    });
    cancelled_band(&shell, cx, start, end);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn fix_band_ey134_list_held_top_edge_scrolls_and_extends_selection(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "list", 80);
    cx.simulate_keystrokes("end");
    cx.run_until_parked();
    cx.simulate_mouse_move(point(px(900.), px(600.)), None, Modifiers::none());
    cx.simulate_event(ScrollWheelEvent {
        position: point(px(900.), px(600.)),
        delta: ScrollDelta::Lines(point(0., -3.)),
        ..Default::default()
    });
    cx.executor().advance_clock(Duration::from_millis(300));
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
    let frame = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().input.frame.unwrap()
    });
    let start = point(
        frame.bounds.right() - px(80.),
        frame.bounds.bottom() - px(3.),
    );
    let end = point(frame.bounds.left() + px(100.), frame.bounds.top() - px(10.));
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.pane.cursor, 79, "End reached the last row");
        assert!(
            host.input.hit(start).is_none(),
            "start in footer ground: {frame:?}, hit {:?}",
            host.input.hit(start)
        );
    });
    drag(cx, start, end);
    let before = shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(
            host.input.band.as_ref().is_some_and(|band| band.active),
            "band remains active above rows"
        );
        host.pane.scroll_y
    });
    for _ in 0..10 {
        cx.executor().advance_clock(Duration::from_millis(100));
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
        });
        cx.run_until_parked();
    }
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(
            host.pane.scroll_y < before - frame.row * 5.,
            "stationary edge hold scrolls multiple rows"
        );
        assert!(
            host.selected.first().is_some_and(|&row| row < 60),
            "selection reaches rows revealed by scrolling"
        );
        assert!(host.input.band.as_ref().is_some_and(|band| band.active));
    });
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| {
        assert!(shell.panes[0].as_ref().unwrap().input.band.is_none())
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn fix_band_grid_pane_not_ready_keeps_no_tile_hit_boxes(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "grid", 10);
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(
            host.input.hit_boxes.is_some(),
            "a Ready grid answers presses on its tiles"
        );
    });
    shell.update(cx, |shell, cx| {
        shell.panes[0].as_mut().unwrap().pane.state.kind = crate::pane::states::Kind::Loading;
        cx.notify();
    });
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(
            host.input.hit_boxes.is_none(),
            "a loading grid draws no tiles, so none answers a press"
        );
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn nav_actions_ey199_down_after_a_ground_click_selects_the_next_row(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "list", 5);
    let frame = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().input.frame.unwrap()
    });
    let first = point(
        frame.bounds.left() + px(100.),
        frame.bounds.top() + px(frame.row / 2.),
    );
    let ground = point(
        frame.bounds.left() + px(100.),
        frame.bounds.bottom() - px(20.),
    );
    for at in [first, ground] {
        cx.simulate_mouse_move(at, None, Modifiers::none());
        cx.simulate_click(at, Modifiers::none());
        cx.run_until_parked();
    }
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.pane.cursor, 0);
        assert!(host.selected.is_empty(), "a ground click clears the marks");
    });
    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.pane.cursor, 1);
        assert_eq!(
            host.selected,
            [1],
            "the next Down selects the row it lights"
        );
    });
    cx.simulate_keystrokes("escape down");
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.pane.cursor, 2);
        assert!(
            host.selected.is_empty(),
            "Escape still leaves a plain move unmarked"
        );
    });
    cx.simulate_click(ground, Modifiers::none());
    cx.run_until_parked();
    cx.simulate_keystrokes("escape down");
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.pane.cursor, 3);
        assert!(
            host.selected.is_empty(),
            "Escape cancels an unconsumed ground click"
        );
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn nav_actions_ey134_row_drag_scrolls_while_held_at_the_top(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "list", 80);
    cx.simulate_keystrokes("end");
    cx.run_until_parked();
    let frame = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().input.frame.unwrap()
    });
    let start = point(
        frame.bounds.left() + px(120.),
        frame.bounds.bottom() - px(frame.row / 2.),
    );
    let end = point(start.x, frame.bounds.top() - px(10.));
    cx.simulate_click(start, Modifiers::none());
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| {
        assert_eq!(shell.panes[0].as_ref().unwrap().selected, [79]);
    });
    drag(cx, start, end);
    assert!(
        cx.update(|_, cx| cx.has_active_drag()),
        "the row lift is active"
    );
    let before = shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(
            host.input.band.is_none(),
            "a row drag keeps its carried selection"
        );
        host.pane.scroll_y
    });
    for _ in 0..10 {
        cx.executor().advance_clock(Duration::from_millis(100));
        cx.update(|window, cx| window.simulate_next_frame(cx));
        cx.run_until_parked();
    }
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(
            host.pane.scroll_y < before - frame.row * 5.,
            "a stationary row drag scrolls at the edge"
        );
        assert_eq!(
            host.selected,
            [79],
            "scrolling never changes the carried row"
        );
    });
    cx.simulate_keystrokes("escape");
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    let stopped = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().pane.scroll_y
    });
    cx.executor().advance_clock(Duration::from_millis(300));
    cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| {
        assert_eq!(shell.panes[0].as_ref().unwrap().pane.scroll_y, stopped)
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[path = "band_noscene_tests.rs"]
mod noscene;
