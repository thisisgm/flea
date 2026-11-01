//! EY-54: a wheel notch moves Grid as far as List, the fork's three lines as one notch of 3 x 24px x 4 (FastScrollHandler).
use super::{join_tests::open, Shown};
use crate::{
    input::scroll::{out_cubic, wheel_delta, RETURN_MS},
    state::Shell,
    window::startup::View,
};
use gpui::{
    point, px, AppContext as _, MouseButton, MouseDownEvent, MouseUpEvent, ScrollDelta,
    ScrollWheelEvent, TestAppContext, WindowHandle,
};
use std::time::Duration;

fn grid(cx: &mut TestAppContext) -> WindowHandle<Shell> {
    let (handle, _) = open(cx);
    handle
        .update(cx, |shell, window, cx| {
            shell.set_view(View::Grid, window, cx)
        })
        .unwrap();
    super::join_tests::frame(cx, handle);
    handle
}

fn notch(cx: &mut TestAppContext, handle: WindowHandle<Shell>, lines: f32) {
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
            let event = ScrollWheelEvent {
                position: at,
                delta: ScrollDelta::Lines(point(0., lines)),
                ..Default::default()
            };
            shell.wheel(&event, window, cx);
        })
        .unwrap();
}

fn scroll_and_max(cx: &mut TestAppContext, handle: WindowHandle<Shell>) -> (f32, f32) {
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            let view = &host.grid.as_ref().unwrap().view;
            (
                view.scroll,
                view.layout(Shown::All(host.pane.held.total))
                    .clamp(f32::MAX),
            )
        })
        .unwrap()
}

#[gpui::test]
fn a_wheel_notch_moves_the_grid_as_far_as_the_list(cx: &mut TestAppContext) {
    let handle = grid(cx);
    let (start, max) = scroll_and_max(cx, handle);
    assert_eq!(start, 0.);
    assert!(
        max > 3. * 288.,
        "the fixture scrolls past three notches, max {max}"
    );
    notch(cx, handle, -3.);
    assert_eq!(
        scroll_and_max(cx, handle).0,
        288.,
        "one notch is the list's 288px"
    );
    notch(cx, handle, -3.);
    notch(cx, handle, -3.);
    assert_eq!(scroll_and_max(cx, handle).0, 864.);
    notch(cx, handle, 3.);
    assert_eq!(
        scroll_and_max(cx, handle).0,
        576.,
        "a notch up comes back as far"
    );
}

fn next_frame(cx: &mut TestAppContext, handle: WindowHandle<Shell>) -> usize {
    cx.update_window(handle.into(), |_, window, cx| {
        window.simulate_next_frame(cx)
    })
    .unwrap()
}

fn drawn(cx: &mut TestAppContext, handle: WindowHandle<Shell>) -> f32 {
    handle
        .update(cx, |shell, _, _| {
            let grid = shell.panes[0].as_ref().unwrap().grid.as_ref().unwrap();
            grid.state.borrow().scroll
        })
        .unwrap()
}

// List's notch glides over RETURN_MS on OutCubic (input/wheel.rs); the Grid paints the same glide.
#[gpui::test]
fn a_grid_notch_glides_on_out_cubic_and_lands(cx: &mut TestAppContext) {
    let handle = grid(cx);
    notch(cx, handle, -3.);
    super::join_tests::frame(cx, handle);
    assert_eq!(
        drawn(cx, handle),
        0.,
        "the first frame draws where the notch began"
    );
    let half = Duration::from_millis((RETURN_MS / 2.) as u64);
    cx.executor().advance_clock(half);
    super::join_tests::frame(cx, handle);
    let mid = drawn(cx, handle);
    let expected = 288. * out_cubic(half.as_secs_f64() * 1000. / RETURN_MS) as f32;
    assert!(
        (mid - expected).abs() < 0.5,
        "halfway draws {mid}, OutCubic says {expected}"
    );
    assert_eq!(
        scroll_and_max(cx, handle).0,
        288.,
        "the scroll is written at once"
    );
    // With the clock held, one-shot frame asks drain away; a running glide asks on every frame.
    for _ in 0..20 {
        cx.run_until_parked();
        assert!(
            next_frame(cx, handle) > 0,
            "a running glide asks for the next frame"
        );
        super::join_tests::frame(cx, handle);
    }
    cx.executor().advance_clock(half * 2);
    super::join_tests::frame(cx, handle);
    assert_eq!(drawn(cx, handle), 288.);
    let quiet = (0..20).any(|_| {
        cx.run_until_parked();
        let asks = next_frame(cx, handle);
        super::join_tests::frame(cx, handle);
        asks == 0
    });
    assert!(quiet, "a landed glide stops asking for frames");
}

// A press mid-glide lands where the paint is, so the tile under the pointer is the one hit; reduced motion never glides.
#[gpui::test]
fn a_press_lands_the_glide_and_reduced_motion_jumps(cx: &mut TestAppContext) {
    let handle = grid(cx);
    notch(cx, handle, -3.);
    super::join_tests::frame(cx, handle);
    cx.executor()
        .advance_clock(Duration::from_millis((RETURN_MS / 2.) as u64));
    super::join_tests::frame(cx, handle);
    let mid = drawn(cx, handle);
    assert!(
        mid > 0. && mid < 288.,
        "the press comes mid-glide, drawn {mid}"
    );
    // The middle drawn tile's slot centre, a point tile_at answers on the drawn frame.
    let (at, under) = handle
        .update(cx, |shell, _, _| {
            let grid = shell.panes[0].as_ref().unwrap().grid.as_ref().unwrap();
            let state = grid.state.borrow();
            let tiles = state.drawn();
            let tile = &tiles[tiles.len() / 2];
            let (x, y) = (
                tile.slot.x + tile.slot.width / 2.,
                tile.slot.y + tile.slot.height / 2.,
            );
            assert_eq!(state.tile_at(x, y), Some(tile.view));
            (state.bounds.origin + point(px(x), px(y)), tile.listing)
        })
        .unwrap();
    handle
        .update(cx, |shell, window, cx| {
            let press = MouseDownEvent {
                button: MouseButton::Left,
                position: at,
                click_count: 1,
                ..Default::default()
            };
            shell.grid_press(0, &press, window, cx);
            assert_eq!(
                shell.panes[0]
                    .as_ref()
                    .unwrap()
                    .input
                    .press
                    .as_ref()
                    .map(|press| press.row),
                Some(under),
                "the press arms the tile drawn under it"
            );
            shell.pointer_up(
                0,
                &MouseUpEvent {
                    button: MouseButton::Left,
                    position: at,
                    ..Default::default()
                },
                window,
                cx,
            );
        })
        .unwrap();
    let cursor = handle
        .update(cx, |shell, _, _| {
            shell.panes[0].as_ref().unwrap().pane.cursor
        })
        .unwrap();
    assert_eq!(
        scroll_and_max(cx, handle).0,
        mid,
        "the press lands the glide where it is drawn"
    );
    assert_eq!(cursor, under, "the press takes the tile drawn under it");
    cx.update(|cx| cx.set_reduce_motion(true));
    notch(cx, handle, -3.);
    super::join_tests::frame(cx, handle);
    assert_eq!(
        drawn(cx, handle),
        mid + 288.,
        "reduced motion draws the notch at once"
    );
}

// GridArea.qml asked for rows from the animated contentY, so mid-glide the held window and thumbnails follow the drawn tiles.
#[gpui::test]
fn mid_glide_the_wanted_rows_follow_what_is_drawn(cx: &mut TestAppContext) {
    let handle = grid(cx);
    notch(cx, handle, -3.);
    notch(cx, handle, -3.);
    notch(cx, handle, -3.);
    super::join_tests::frame(cx, handle);
    let (wanted, target) = handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            let view = &host.grid.as_ref().unwrap().view;
            let shown = Shown::All(host.pane.held.total);
            let wanted = view.wanted_thumbnails(&host.pane, shown).unwrap();
            (*wanted.start(), view.layout(shown).viewport(view.scroll).0)
        })
        .unwrap();
    assert_eq!(drawn(cx, handle), 0.);
    assert!(target > 0, "the target viewport starts below the top");
    assert_eq!(
        wanted, 0,
        "thumbnails cover the drawn top row, not the target's row {target}"
    );
}

// A fast flick far past the held rows: the window is asked for where the glide is drawn, never for the target first.
#[gpui::test]
fn mid_glide_the_held_window_is_asked_where_it_is_drawn(cx: &mut TestAppContext) {
    let handle = grid(cx);
    let asks = |cx: &mut TestAppContext| {
        handle
            .update(cx, |shell, _, _| {
                let grid = shell.panes[0].as_ref().unwrap().grid.as_ref().unwrap();
                grid.view.counters.drift_asks
            })
            .unwrap()
    };
    let before = asks(cx);
    for _ in 0..15 {
        notch(cx, handle, -3.);
    }
    super::join_tests::frame(cx, handle);
    let (held, target) = handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            let view = &host.grid.as_ref().unwrap().view;
            let shown = Shown::All(host.pane.held.total);
            let held = host.pane.held.start + host.pane.held.rows.len();
            (held, view.layout(shown).viewport(view.scroll).0)
        })
        .unwrap();
    assert_eq!(drawn(cx, handle), 0.);
    assert!(
        target as usize > held,
        "the target row {target} lies past the held rows ending at {held}"
    );
    assert_eq!(
        asks(cx),
        before,
        "no window is asked for the undrawn target"
    );
    // Halfway the drawn rows near the end of the held ones, so the ask comes mid-glide, as contentY's did.
    cx.executor()
        .advance_clock(Duration::from_millis((RETURN_MS / 2.) as u64));
    super::join_tests::frame(cx, handle);
    let first = handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            let view = &host.grid.as_ref().unwrap().view;
            let shown = Shown::All(host.pane.held.total);
            view.layout(shown).viewport(view.drawn_scroll()).0
        })
        .unwrap();
    assert!(
        first > 0 && first < target,
        "mid-glide the drawn row {first} lies between the top and the target row {target}"
    );
    assert_eq!(
        asks(cx),
        before + 1,
        "the window is asked mid-glide for the drawn rows"
    );
}

// The previews share the conversion: a notch is 288px on either axis, a touchpad delta passes through.
#[test]
fn the_wheel_delta_is_the_list_notch_for_every_view() {
    let notch = wheel_delta(&ScrollDelta::Lines(point(1.5, -3.)));
    assert_eq!((f32::from(notch.x), f32::from(notch.y)), (144., -288.));
    let pixels = wheel_delta(&ScrollDelta::Pixels(point(px(7.), px(-11.))));
    assert_eq!((f32::from(pixels.x), f32::from(pixels.y)), (7., -11.));
}

#[path = "nav_actions_tests.rs"]
mod nav_actions;
