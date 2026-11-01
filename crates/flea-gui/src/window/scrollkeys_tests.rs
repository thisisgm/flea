//! Mounted keyboard and scrollbar regressions over the same long directory.
use super::{columns_tests::host, startup::View};
use crate::{columns::geometry::Slot, pane::Held, state::Shell};
use gpui::{point, px, Modifiers, MouseButton, TestAppContext, VisualTestContext, WindowHandle};
use std::rc::Rc;

pub(super) fn mounted(cx: &mut TestAppContext, view: View) -> WindowHandle<Shell> {
    let (handle, _requests) = host::open_at(cx, "/srv/a/b/c", 1100., 3);
    host::change(cx, handle, |shell, window, cx| {
        let pane = &mut shell.panes[0].as_mut().unwrap().pane;
        pane.held = Rc::new(Held {
            listing: 3,
            total: 80,
            start: 0,
            rows: (1..=80)
                .map(|index| host::row(&format!("item-{index:02}.txt"), false))
                .collect(),
        });
        pane.scroll_extent = None;
        host::settle(shell, window, cx);
        shell.set_view(view, window, cx);
    });
    host::frame(cx, handle);
    handle
}

fn key(cx: &mut TestAppContext, handle: WindowHandle<Shell>, key: &str) {
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes(key);
    host::frame(cx, handle);
}

#[test]
fn scrollkeys_columns_page_keys_keep_half_page_steps_and_three_context_rows() {
    let mut cx = host::context();
    let handle = mounted(&mut cx, View::Columns);
    key(&mut cx, handle, "pagedown");
    host::change(&mut cx, handle, |shell, window, cx| {
        assert_eq!(
            host::call(shell, "cursor", &[], window, cx).stdout.trim(),
            "10"
        );
        assert_eq!(shell.panes[0].as_ref().unwrap().pane.scroll_y, 0.);
    });
    key(&mut cx, handle, "pagedown");
    host::change(&mut cx, handle, |shell, _, _| {
        let pane = shell.panes[0].as_ref().unwrap();
        assert_eq!(pane.pane.cursor, 20);
        let columns = pane.columns.as_ref().unwrap();
        let paint = columns.painters.get(Slot::Active).unwrap().borrow();
        let drawn = paint.drawn.as_ref().unwrap();
        let row = crate::pane::ROW_HEIGHT;
        let bottom = (pane.pane.cursor + 4) as f32 * row - drawn.scroll;
        assert!(
            bottom <= f32::from(drawn.bounds.size.height),
            "three whole rows below cursor"
        );
    });
    key(&mut cx, handle, "pageup");
    host::change(&mut cx, handle, |shell, _, _| {
        assert_eq!(shell.panes[0].as_ref().unwrap().pane.cursor, 10);
    });
    key(&mut cx, handle, "home pageup");
    host::change(&mut cx, handle, |shell, _, _| {
        let pane = &shell.panes[0].as_ref().unwrap().pane;
        assert_eq!((pane.cursor, pane.scroll_y), (0, 0.));
    });
}

#[test]
fn scrollkeys_grid_lane_drag_scrolls_and_paints_the_shared_bar() {
    let mut cx = host::context();
    let handle = mounted(&mut cx, View::Grid);
    let bounds = handle
        .update(&mut cx, |shell, _, _| {
            shell.panes[0]
                .as_ref()
                .unwrap()
                .grid
                .as_ref()
                .unwrap()
                .state
                .borrow()
                .bounds
        })
        .unwrap();
    let at = |fraction| {
        point(
            bounds.right() - px(3.),
            bounds.top() + bounds.size.height * fraction,
        )
    };
    let (start, end) = (at(0.60), at(0.35));
    {
        let mut visual = VisualTestContext::from_window(handle.into(), &cx);
        visual.simulate_mouse_move(start, None, Modifiers::none());
        visual.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    }
    host::frame(&mut cx, handle);
    VisualTestContext::from_window(handle.into(), &cx).simulate_mouse_move(
        end,
        Some(MouseButton::Left),
        Modifiers::none(),
    );
    host::frame(&mut cx, handle);
    host::change(&mut cx, handle, |shell, window, cx| {
        let host = shell.panes[0].as_ref().unwrap();
        let grid = host.grid.as_ref().unwrap();
        assert_eq!(
            host.input.drawn.unwrap().frame.bounds,
            grid.state.borrow().bounds
        );
        assert!(
            grid.state.borrow().scroll > 0.,
            "drag must move the painted grid"
        );
        assert!(grid.state.borrow().drawn()[0].listing > 0);
        assert_eq!(host.pane.scroll_y, 0., "Grid keeps its own scroll");
        let bar = host::call(shell, "scrollbarState", &[], window, cx);
        let state = flea::jsondoc::parse(bar.stdout.trim()).unwrap();
        assert_eq!(
            state.get("shown").and_then(flea::jsondoc::Json::as_bool),
            Some(true)
        );
    });
    VisualTestContext::from_window(handle.into(), &cx).simulate_mouse_up(
        end,
        MouseButton::Left,
        Modifiers::none(),
    );
    host::frame(&mut cx, handle);
    host::change(&mut cx, handle, |shell, _, _| {
        assert!(shell.panes[0].as_ref().unwrap().input.grab.is_none());
    });
}

#[test]
fn scrollkeys_grid_bar_keeps_its_grab_outside_and_releases_without_selecting() {
    let mut cx = host::context();
    let handle = mounted(&mut cx, View::Grid);
    let bounds = handle
        .update(&mut cx, |shell, _, _| {
            shell.panes[0]
                .as_ref()
                .unwrap()
                .grid
                .as_ref()
                .unwrap()
                .state
                .borrow()
                .bounds
        })
        .unwrap();
    let start = point(bounds.right() - px(3.), bounds.top() + px(30.));
    let outside = point(bounds.right() + px(100.), bounds.bottom() + px(800.));
    {
        let mut visual = VisualTestContext::from_window(handle.into(), &cx);
        visual.simulate_mouse_move(start, None, Modifiers::none());
        visual.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    }
    host::frame(&mut cx, handle);
    VisualTestContext::from_window(handle.into(), &cx).simulate_mouse_move(
        outside,
        Some(MouseButton::Left),
        Modifiers::none(),
    );
    host::frame(&mut cx, handle);
    let scroll = handle
        .update(&mut cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            let grid = host.grid.as_ref().unwrap();
            let state = grid.state.borrow();
            assert_eq!(state.scroll, state.layout.unwrap().max_scroll());
            assert_eq!(host.pane.cursor, 0);
            assert!(host.selected.is_empty());
            assert!(host.input.grab.is_some());
            state.scroll
        })
        .unwrap();
    VisualTestContext::from_window(handle.into(), &cx).simulate_mouse_up(
        outside,
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    VisualTestContext::from_window(handle.into(), &cx).simulate_mouse_move(
        start,
        None,
        Modifiers::none(),
    );
    host::frame(&mut cx, handle);
    host::change(&mut cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(host.input.grab.is_none());
        assert!(!host.nav.gate.pointer_held);
        assert_eq!(host.grid.as_ref().unwrap().state.borrow().scroll, scroll);
    });
}

#[test]
fn scrollkeys_grid_error_ends_the_scrollbar_grab_and_pointer_hold() {
    let mut cx = host::context();
    let handle = mounted(&mut cx, View::Grid);
    let at = handle
        .update(&mut cx, |shell, _, _| {
            let grid = shell.panes[0].as_ref().unwrap().grid.as_ref().unwrap();
            let bounds = grid.state.borrow().bounds;
            point(bounds.right() - px(3.), bounds.top() + px(30.))
        })
        .unwrap();
    {
        let mut visual = VisualTestContext::from_window(handle.into(), &cx);
        visual.simulate_mouse_move(at, None, Modifiers::none());
        visual.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    }
    host::frame(&mut cx, handle);
    host::change(&mut cx, handle, |shell, _, cx| {
        let host = shell.panes[0].as_mut().unwrap();
        assert!(host.input.grab.is_some());
        host.pane.state.kind = crate::pane::states::Kind::Error;
        cx.notify();
    });
    host::frame(&mut cx, handle);
    host::change(&mut cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(host.input.grab.is_none());
        assert!(!host.nav.gate.pointer_held);
        assert!(host.input.drawn.is_none());
    });
}

// case_shiftscroll and click_row: a rowCentre is a point a click lands on, so a part-drawn bottom row answers none.
fn centres_stay_inside_the_list_area(view: View) {
    let mut cx = host::context();
    let handle = mounted(&mut cx, view);
    // Sample input: "192 54 908 619" (x y width height).
    fn numbers(
        shell: &mut Shell,
        function: &str,
        args: &[&str],
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Shell>,
    ) -> Vec<i64> {
        host::call(shell, function, args, window, cx)
            .stdout
            .split_whitespace()
            .map(|n| n.parse().unwrap())
            .collect()
    }
    // Scroll so row 30's top shows only 4 px above the area's bottom edge.
    host::change(&mut cx, handle, |shell, window, cx| {
        let area = numbers(shell, "listAreaRect", &[], window, cx);
        let row = numbers(shell, "rowRect", &["0"], window, cx);
        let scroll = (30 * row[3] - area[3] + 4) as f32;
        shell.panes[0].as_mut().unwrap().pane.scroll_y = scroll;
        cx.notify();
    });
    host::frame(&mut cx, handle);
    host::change(&mut cx, handle, |shell, window, cx| {
        let area = numbers(shell, "listAreaRect", &[], window, cx);
        let edge = numbers(shell, "rowRect", &["30"], window, cx);
        assert_eq!(edge.len(), 4, "row 30 is drawn part way at the bottom edge");
        assert!(edge[1] < area[1] + area[3] && edge[1] + edge[3] > area[1] + area[3]);
        assert_eq!(
            host::call(shell, "rowCentre", &["30"], window, cx)
                .stdout
                .trim(),
            ""
        );
        for index in 0..80 {
            let centre = numbers(shell, "rowCentre", &[&index.to_string()], window, cx);
            if let [x, y] = centre[..] {
                assert!(
                    x >= area[0] && x < area[0] + area[2] && y >= area[1] && y < area[1] + area[3],
                    "row {index}'s centre {x},{y} lies outside {area:?}"
                );
            }
        }
        assert_eq!(
            numbers(shell, "rowCentre", &["29"], window, cx).len(),
            2,
            "row 29 is whole"
        );
    });
}

#[test]
fn ey444_list_row_centres_stay_inside_the_list_area() {
    centres_stay_inside_the_list_area(View::List);
}

#[test]
fn ey444_columns_row_centres_stay_inside_the_list_area() {
    centres_stay_inside_the_list_area(View::Columns);
}
