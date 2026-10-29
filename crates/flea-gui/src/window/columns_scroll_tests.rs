//! Each mounted Columns track owns a wheel stroke and a draggable overlay lane.
use super::host;
use crate::{columns::geometry::Slot, list::pane::Held, state::Shell};
use gpui::{
    point, px, Modifiers, MouseButton, Pixels, Point, ScrollDelta, ScrollWheelEvent,
    TestAppContext, TouchPhase, VisualTestContext, WindowHandle,
};
use std::{rc::Rc, time::Duration};

pub(crate) fn mounted(cx: &mut TestAppContext) -> WindowHandle<Shell> {
    let (handle, _requests) = host::open_at(cx, "/srv/a/b/c", 2700., 5);
    host::change(cx, handle, |shell, window, cx| {
        shell.panes[0].as_mut().unwrap().pane.held = Rc::new(Held {
            listing: 3,
            total: 256,
            start: 0,
            rows: (0..256)
                .map(|i| host::row(if i == 0 { "d01" } else { "file.txt" }, i == 0))
                .collect(),
        });
        for (path, name) in [
            ("/srv", "a"),
            ("/srv/a", "b"),
            ("/srv/a/b", "c"),
            ("/srv/a/b/c/d01", "seed"),
        ] {
            let rows = vec![(name, true); 256];
            host::peek(shell, path, &rows, false, window, cx);
        }
    });
    host::frame(cx, handle);
    handle
}

pub(crate) fn bounds(
    cx: &mut TestAppContext,
    handle: WindowHandle<Shell>,
    slot: Slot,
) -> gpui::Bounds<Pixels> {
    handle
        .update(cx, |shell, _, _| {
            shell.panes[0]
                .as_ref()
                .unwrap()
                .columns
                .as_ref()
                .unwrap()
                .painters
                .get(slot)
                .unwrap()
                .borrow()
                .drawn
                .as_ref()
                .unwrap()
                .bounds
        })
        .unwrap()
}

pub(crate) fn scroll(cx: &mut TestAppContext, handle: WindowHandle<Shell>, slot: Slot) -> f32 {
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            if slot == Slot::Active {
                host.pane.scroll_y
            } else {
                host.columns.as_ref().unwrap().area.scroll(slot)
            }
        })
        .unwrap()
}

pub(crate) fn wheel(
    cx: &TestAppContext,
    handle: WindowHandle<Shell>,
    at: Point<Pixels>,
    phase: TouchPhase,
    y: f32,
) {
    VisualTestContext::from_window(handle.into(), cx).simulate_event(ScrollWheelEvent {
        position: at,
        delta: ScrollDelta::Pixels(point(px(0.), px(y))),
        touch_phase: phase,
        ..Default::default()
    });
}

#[test]
fn pb_scroll_columns_each_lane_drag_scrolls_only_its_track() {
    let mut cx = host::context();
    let handle = mounted(&mut cx);
    for slot in [
        Slot::GreatGrandparent,
        Slot::Grandparent,
        Slot::Parent,
        Slot::Active,
        Slot::Child,
    ] {
        let b = bounds(&mut cx, handle, slot);
        let at = point(b.right() - px(6.), b.top() + px(400.));
        let before = scroll(&mut cx, handle, slot);
        let others = [
            Slot::GreatGrandparent,
            Slot::Grandparent,
            Slot::Parent,
            Slot::Active,
            Slot::Child,
        ]
        .map(|other| (other, scroll(&mut cx, handle, other)));
        let selection = handle
            .update(&mut cx, |shell, _, _| {
                let host = shell.panes[0].as_ref().unwrap();
                (host.pane.cursor, host.selected.clone())
            })
            .unwrap();
        let mut visual = VisualTestContext::from_window(handle.into(), &cx);
        visual.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
        visual.simulate_mouse_move(
            point(at.x, b.bottom() + px(100.)),
            Some(MouseButton::Left),
            Modifiers::none(),
        );
        visual.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
        assert!(
            scroll(&mut cx, handle, slot) > before,
            "{slot:?} lane must drag its own scroll"
        );
        for (other, y) in others {
            if other != slot {
                assert_eq!(scroll(&mut cx, handle, other), y)
            }
        }
        handle
            .update(&mut cx, |shell, _, _| {
                let host = shell.panes[0].as_ref().unwrap();
                assert_eq!((host.pane.cursor, host.selected.clone()), selection);
            })
            .unwrap();
        host::frame(&mut cx, handle);
    }
}

#[test]
fn pb_scroll_columns_each_touchpad_lift_coasts() {
    let mut cx = host::context();
    let handle = mounted(&mut cx);
    for slot in [Slot::Parent, Slot::Active, Slot::Child] {
        let at = bounds(&mut cx, handle, slot).center();
        wheel(&cx, handle, at, TouchPhase::Started, 0.);
        for _ in 0..6 {
            cx.executor().advance_clock(Duration::from_millis(8));
            wheel(&cx, handle, at, TouchPhase::Moved, -24.);
            host::frame(&mut cx, handle);
        }
        let lifted = scroll(&mut cx, handle, slot);
        wheel(&cx, handle, at, TouchPhase::Ended, 0.);
        for _ in 0..3 {
            cx.executor().advance_clock(Duration::from_millis(16));
            host::frame(&mut cx, handle);
        }
        assert!(
            scroll(&mut cx, handle, slot) > lifted,
            "{slot:?} tail must move after lift"
        );
    }
}
