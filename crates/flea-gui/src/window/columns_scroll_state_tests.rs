//! Mounted motion and reveal states, including cancellation when the painted listing changes.
use crate::{
    columns::geometry::Slot,
    window::columns_tests::{
        host,
        scroll::{bounds, mounted, scroll, wheel},
    },
};
use gpui::{
    point, px, Modifiers, MouseButton, ScrollDelta, ScrollWheelEvent, TouchPhase, VisualTestContext,
};
use std::{rc::Rc, time::Duration};

#[test]
fn pb_scroll_columns_bar_is_space_at_rest_shown_on_use_and_fades() {
    let mut cx = host::context();
    let handle = mounted(&mut cx);
    let b = bounds(&mut cx, handle, Slot::Active);
    host::change(&mut cx, handle, |shell, _, cx| {
        let input = &shell.panes[0]
            .as_ref()
            .unwrap()
            .columns
            .as_ref()
            .unwrap()
            .scroll[3]
            .input;
        assert_eq!(input.opacity(cx.background_executor().now(), false), 0.);
        assert!(!input.drawn.unwrap().shown);
    });
    let lane = point(b.right() - px(6.), b.center().y);
    VisualTestContext::from_window(handle.into(), &cx).simulate_mouse_move(
        lane,
        None,
        Modifiers::none(),
    );
    host::frame(&mut cx, handle);
    host::change(&mut cx, handle, |shell, _, _| {
        assert!(
            shell.panes[0]
                .as_ref()
                .unwrap()
                .columns
                .as_ref()
                .unwrap()
                .scroll[3]
                .input
                .drawn
                .unwrap()
                .shown
        );
    });
    VisualTestContext::from_window(handle.into(), &cx).simulate_mouse_move(
        b.center(),
        None,
        Modifiers::none(),
    );
    VisualTestContext::from_window(handle.into(), &cx).simulate_event(ScrollWheelEvent {
        position: b.center(),
        delta: ScrollDelta::Lines(point(0., -3.)),
        ..Default::default()
    });
    assert_eq!(scroll(&mut cx, handle, Slot::Active), 288.);
    cx.executor().advance_clock(Duration::from_millis(90));
    host::frame(&mut cx, handle);
    host::change(&mut cx, handle, |shell, _, _| {
        let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
        let paint = columns.painters.get(Slot::Active).unwrap().borrow();
        let shown = paint.drawn.as_ref().unwrap().scroll;
        assert!(
            (shown - 252.).abs() < 1.,
            "the notch draws OutCubic's half-time position"
        );
        assert!(columns.scroll[3].input.drawn.unwrap().shown);
    });
    cx.executor().advance_clock(Duration::from_millis(1400));
    host::frame(&mut cx, handle);
    host::change(&mut cx, handle, |shell, _, cx| {
        let input = &shell.panes[0]
            .as_ref()
            .unwrap()
            .columns
            .as_ref()
            .unwrap()
            .scroll[3]
            .input;
        assert_eq!(input.opacity(cx.background_executor().now(), false), 0.);
        assert!(!input.drawn.unwrap().shown);
    });
}

#[test]
fn pb_scroll_columns_rubber_band_returns_and_cancelled_strokes_do_not_coast() {
    let mut cx = host::context();
    let handle = mounted(&mut cx);
    for slot in [Slot::Parent, Slot::Active, Slot::Child] {
        let at = bounds(&mut cx, handle, slot).center();
        wheel(&cx, handle, at, TouchPhase::Started, 0.);
        wheel(&cx, handle, at, TouchPhase::Moved, 120.);
        host::frame(&mut cx, handle);
        assert_eq!(scroll(&mut cx, handle, slot), 0.);
        host::change(&mut cx, handle, |shell, _, _| {
            let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
            assert!(
                columns
                    .painters
                    .get(slot)
                    .unwrap()
                    .borrow()
                    .drawn
                    .as_ref()
                    .unwrap()
                    .scroll
                    < 0.
            );
        });
        cx.executor().advance_clock(Duration::from_millis(200));
        wheel(&cx, handle, at, TouchPhase::Ended, 0.);
        for _ in 0..12 {
            cx.executor().advance_clock(Duration::from_millis(16));
            host::frame(&mut cx, handle);
        }
        host::change(&mut cx, handle, |shell, _, _| {
            let input = &shell.panes[0]
                .as_ref()
                .unwrap()
                .columns
                .as_ref()
                .unwrap()
                .scroll[slot.index()]
            .input;
            assert_eq!(input.over, 0.);
            assert!(!input.moving());
        });
        wheel(&cx, handle, at, TouchPhase::Started, 0.);
        wheel(&cx, handle, at, TouchPhase::Moved, -24.);
        wheel(&cx, handle, at, TouchPhase::Cancelled, 0.);
        let held = scroll(&mut cx, handle, slot);
        cx.executor().advance_clock(Duration::from_millis(32));
        host::frame(&mut cx, handle);
        assert_eq!(scroll(&mut cx, handle, slot), held);
    }
}

#[test]
fn pb_scroll_columns_listing_swap_cancels_lane_grab_and_watch_hold() {
    let mut cx = host::context();
    let handle = mounted(&mut cx);
    let b = bounds(&mut cx, handle, Slot::Active);
    let at = point(b.right() - px(6.), b.top() + px(100.));
    VisualTestContext::from_window(handle.into(), &cx).simulate_mouse_down(
        at,
        MouseButton::Left,
        Modifiers::none(),
    );
    host::change(&mut cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_mut().unwrap();
        assert!(host.columns.as_ref().unwrap().scroll[3]
            .input
            .grab
            .is_some());
        assert!(host.nav.gate.pointer_held);
        host.pane.held = Rc::new(crate::list::pane::Held {
            listing: host.pane.held.listing + 1,
            total: host.pane.held.total,
            start: host.pane.held.start,
            rows: host.pane.held.rows.clone(),
        });
    });
    host::frame(&mut cx, handle);
    host::change(&mut cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(host.columns.as_ref().unwrap().scroll[3]
            .input
            .grab
            .is_none());
        assert!(!host.nav.gate.pointer_held);
    });
}
