//! Mounted Columns scrollbar delivery, with every directory track owning its position.
use super::super::columns_tests::host;
use crate::{columns::geometry::Slot, list::pane::Held, state::Shell};
use gpui::{
    point, px, Bounds, Modifiers, MouseButton, Pixels, ScrollDelta, ScrollWheelEvent,
    TestAppContext, VisualTestContext, WindowHandle,
};
use std::{rc::Rc, time::Duration};

const TOTAL: usize = 256;

fn mounted(
    cx: &mut TestAppContext,
) -> (WindowHandle<Shell>, impl FnMut() -> Option<(usize, usize)>) {
    let (handle, requests) = host::open_at(cx, "/srv/a/b/c", 2700., 5);
    host::change(cx, handle, |shell, window, cx| {
        let pane = &mut shell.panes[0].as_mut().unwrap().pane;
        pane.held = Rc::new(Held {
            listing: 3,
            total: TOTAL,
            start: 0,
            rows: (0..TOTAL)
                .map(|i| host::row(&format!("d{:02}", i + 1), true))
                .collect(),
        });
        pane.scroll_extent = None;
        for (path, lifted) in [
            ("/srv", "a"),
            ("/srv/a", "b"),
            ("/srv/a/b", "c"),
            ("/srv/a/b/c/d01", "seed.txt"),
        ] {
            let names: Vec<_> = (0..TOTAL)
                .map(|i| {
                    if i == 0 {
                        lifted.to_owned()
                    } else {
                        format!("file-{i}")
                    }
                })
                .collect();
            let rows: Vec<_> = names.iter().map(|name| (name.as_str(), false)).collect();
            host::peek(shell, path, &rows, false, window, cx);
        }
    });
    host::frame(cx, handle);
    (handle, requests)
}

fn bounds(cx: &mut TestAppContext, handle: WindowHandle<Shell>, slot: Slot) -> Bounds<Pixels> {
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

fn scrolls(cx: &mut TestAppContext, handle: WindowHandle<Shell>) -> [f32; 5] {
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            let columns = host.columns.as_ref().unwrap();
            std::array::from_fn(|i| {
                if i == Slot::Active.index() {
                    host.pane.scroll_y
                } else {
                    columns.area.scroll(
                        [
                            Slot::GreatGrandparent,
                            Slot::Grandparent,
                            Slot::Parent,
                            Slot::Active,
                            Slot::Child,
                        ][i],
                    )
                }
            })
        })
        .unwrap()
}

#[test]
fn pb_scroll_column_lane_jumps_each_track_without_selecting_or_navigating() {
    for slot in [
        Slot::GreatGrandparent,
        Slot::Grandparent,
        Slot::Parent,
        Slot::Active,
        Slot::Child,
    ] {
        let mut cx = host::context();
        let (handle, _requests) = mounted(&mut cx);
        let rect = bounds(&mut cx, handle, slot);
        let at = point(rect.right() - px(3.), rect.center().y);
        let before = scrolls(&mut cx, handle);
        VisualTestContext::from_window(handle.into(), &cx).simulate_mouse_down(
            at,
            MouseButton::Left,
            Modifiers::none(),
        );
        host::frame(&mut cx, handle);
        let after = scrolls(&mut cx, handle);
        assert!(
            after[slot.index()] > 1000.,
            "{slot:?}: the lane jumps to its clicked position: {after:?}"
        );
        for i in 0..5 {
            if i != slot.index() {
                assert_eq!(after[i], before[i], "another track moved");
            }
        }
        host::change(&mut cx, handle, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.pane.path(), "/srv/a/b/c");
            assert_eq!(host.pane.cursor, 0);
            assert!(host.selected.is_empty());
        });
    }
}

#[test]
fn pb_scroll_column_knob_keeps_drag_outside_and_releases() {
    let mut cx = host::context();
    let (handle, _requests) = mounted(&mut cx);
    let slot = Slot::Active;
    let rect = bounds(&mut cx, handle, slot);
    let at = point(rect.right() - px(3.), rect.top() + px(8.));
    let outside = point(rect.right() + px(100.), rect.bottom() + px(800.));
    VisualTestContext::from_window(handle.into(), &cx).simulate_mouse_down(
        at,
        MouseButton::Left,
        Modifiers::none(),
    );
    VisualTestContext::from_window(handle.into(), &cx).simulate_mouse_move(
        outside,
        Some(MouseButton::Left),
        Modifiers::none(),
    );
    host::frame(&mut cx, handle);
    let maximum = TOTAL as f32 * crate::list::pane::ROW_HEIGHT - f32::from(rect.size.height);
    assert!((scrolls(&mut cx, handle)[slot.index()] - maximum).abs() < 1.);
    VisualTestContext::from_window(handle.into(), &cx).simulate_mouse_up(
        outside,
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    VisualTestContext::from_window(handle.into(), &cx).simulate_mouse_move(
        at,
        None,
        Modifiers::none(),
    );
    host::frame(&mut cx, handle);
    assert!(
        (scrolls(&mut cx, handle)[slot.index()] - maximum).abs() < 1.,
        "released drag must not follow a later move"
    );
}

fn visibility(cx: &mut TestAppContext, handle: WindowHandle<Shell>, slot: Slot) -> (bool, f32) {
    handle
        .update(cx, |shell, _, cx| {
            let input = &shell.panes[0]
                .as_ref()
                .unwrap()
                .columns
                .as_ref()
                .unwrap()
                .bars[slot.index()]
            .input;
            let drawn = input.drawn.expect("mounted bar records its paint geometry");
            (
                drawn.shown,
                input.opacity(cx.background_executor().now(), cx.reduce_motion()),
            )
        })
        .unwrap()
}

#[test]
fn pb_scroll_column_lane_is_empty_until_hover_or_motion_and_fades() {
    let mut cx = host::context();
    let (handle, _requests) = mounted(&mut cx);
    let slot = Slot::Active;
    let rect = bounds(&mut cx, handle, slot);
    assert_eq!(visibility(&mut cx, handle, slot), (false, 0.));
    let lane = point(rect.right() - px(3.), rect.center().y);
    VisualTestContext::from_window(handle.into(), &cx).simulate_mouse_move(
        lane,
        None,
        Modifiers::none(),
    );
    host::frame(&mut cx, handle);
    assert_eq!(visibility(&mut cx, handle, slot), (true, 1.));
    let at = rect.center();
    VisualTestContext::from_window(handle.into(), &cx).simulate_mouse_move(
        at,
        None,
        Modifiers::none(),
    );
    host::frame(&mut cx, handle);
    assert_eq!(visibility(&mut cx, handle, slot), (false, 0.));
    VisualTestContext::from_window(handle.into(), &cx).simulate_event(ScrollWheelEvent {
        position: at,
        delta: ScrollDelta::Lines(point(0., -3.)),
        ..Default::default()
    });
    host::frame(&mut cx, handle);
    assert!(scrolls(&mut cx, handle)[slot.index()] > 0.);
    assert_eq!(visibility(&mut cx, handle, slot), (true, 1.));
    cx.executor().advance_clock(Duration::from_millis(1150));
    host::frame(&mut cx, handle);
    assert_eq!(visibility(&mut cx, handle, slot), (false, 0.5));
    cx.executor().advance_clock(Duration::from_millis(150));
    host::frame(&mut cx, handle);
    assert_eq!(visibility(&mut cx, handle, slot), (false, 0.));
}

#[test]
fn pb_scroll_column_resize_cancels_grab_and_releases_watched_rows() {
    let mut cx = host::context();
    let (handle, _requests) = mounted(&mut cx);
    let rect = bounds(&mut cx, handle, Slot::Active);
    let at = point(rect.right() - px(3.), rect.top() + px(8.));
    VisualTestContext::from_window(handle.into(), &cx).simulate_mouse_down(
        at,
        MouseButton::Left,
        Modifiers::none(),
    );
    host::change(&mut cx, handle, |shell, _, _| {
        assert!(shell.panes[0].as_ref().unwrap().nav.gate.pointer_held);
        shell.launch.settings.columns = 3;
    });
    host::frame(&mut cx, handle);
    host::change(&mut cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(host.columns.as_ref().unwrap().bars[Slot::Active.index()]
            .input
            .grab
            .is_none());
        assert!(!host.nav.gate.pointer_held);
    });
}
