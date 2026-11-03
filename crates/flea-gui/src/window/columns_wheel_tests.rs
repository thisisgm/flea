//! Columns wheel delivery through mounted tracks, including the trailing directory list.
use super::host;
use crate::{columns::geometry::Slot, list::pane::Held, state::Shell, window::startup::View};
use gpui::{
    point, px, Pixels, Point, ScrollDelta, ScrollWheelEvent, TestAppContext, TouchPhase,
    VisualTestContext, WindowHandle,
};
use std::rc::Rc;

// ColumnPane.qml uses FastScrollHandler: three platform lines times Theme.scroll's 24 px times four.
const NOTCH_PX: f32 = 3. * 24. * 4.;
const TOTAL: usize = 256;

fn mounted(cx: &mut TestAppContext) -> WindowHandle<Shell> {
    let (handle, _requests) = host::open_at(cx, "/srv/a/b/c", 2700., 5);
    host::change(cx, handle, |shell, window, cx| {
        let pane = &mut shell.panes[0].as_mut().unwrap().pane;
        pane.held = Rc::new(Held {
            listing: 3,
            total: TOTAL,
            start: 0,
            rows: (0..TOTAL)
                .map(|index| {
                    if index == 0 {
                        host::row("d01", true)
                    } else {
                        host::row(&format!("file-{index}.txt"), false)
                    }
                })
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
                .map(|index| {
                    if index == 0 {
                        lifted.to_owned()
                    } else {
                        format!("file-{index}.txt")
                    }
                })
                .collect();
            let rows: Vec<_> = names.iter().map(|name| (name.as_str(), false)).collect();
            host::peek(shell, path, &rows, false, window, cx);
        }
    });
    host::frame(cx, handle);
    handle
}

fn inside(cx: &mut TestAppContext, handle: WindowHandle<Shell>, slot: Slot) -> Point<Pixels> {
    let mut position = None;
    host::change(cx, handle, |shell, _, _| {
        let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
        let paint = columns.painters.get(slot).unwrap().borrow();
        let drawn = paint.drawn.as_ref().expect("the track is mounted");
        assert!(drawn.view().enabled);
        position = Some(drawn.bounds.center());
    });
    position.unwrap()
}

fn send(
    cx: &mut TestAppContext,
    handle: WindowHandle<Shell>,
    at: Point<Pixels>,
    delta: ScrollDelta,
    touch_phase: TouchPhase,
) {
    VisualTestContext::from_window(handle.into(), cx).simulate_event(ScrollWheelEvent {
        position: at,
        delta,
        touch_phase,
        ..Default::default()
    });
}

fn scrolls(cx: &mut TestAppContext, handle: WindowHandle<Shell>) -> [f32; 5] {
    let mut scrolls = [0.; 5];
    host::change(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        let columns = host.columns.as_ref().unwrap();
        for view in columns.area.tracks().iter() {
            scrolls[view.slot.index()] = if view.slot == Slot::Active {
                host.pane.scroll_y
            } else {
                columns.area.scroll(view.slot)
            };
        }
    });
    scrolls
}

fn notches_match_reference(slot: Slot) {
    let mut cx = host::context();
    let handle = mounted(&mut cx);
    let at = inside(&mut cx, handle, slot);
    assert_eq!(scrolls(&mut cx, handle), [0.; 5]);
    for count in 1..=4 {
        send(
            &mut cx,
            handle,
            at,
            ScrollDelta::Lines(point(0., -3.)),
            TouchPhase::Moved,
        );
        let mut expected = [0.; 5];
        expected[slot.index()] = count as f32 * NOTCH_PX;
        assert_eq!(
            scrolls(&mut cx, handle),
            expected,
            "{slot:?}, {count} notches"
        );
    }
    for lines in [0.75, 0.75, 1.5] {
        send(
            &mut cx,
            handle,
            at,
            ScrollDelta::Lines(point(0., lines)),
            TouchPhase::Moved,
        );
    }
    assert_eq!(scrolls(&mut cx, handle)[slot.index()], 3. * NOTCH_PX);
    host::frame(&mut cx, handle);
    // Columns glides each notch as List does, so the paint settles after Omarchy's return duration.
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(
            crate::input::scroll::RETURN_MS as u64,
        ));
    host::frame(&mut cx, handle);
    host::change(&mut cx, handle, |shell, _, _| {
        let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
        let paint = columns.painters.get(slot).unwrap().borrow();
        let drawn = paint.drawn.as_ref().unwrap();
        assert_eq!(
            drawn.scroll,
            3. * NOTCH_PX,
            "settles on v0.3.12's notch target"
        );
        assert_eq!(
            paint.visible().next(),
            Some((3. * NOTCH_PX / crate::list::pane::ROW_HEIGHT).floor() as usize)
        );
    });
}

#[test]
fn colwheel_trailing_track_notches_match_v0312() {
    notches_match_reference(Slot::Child);
}

#[test]
fn colwheel_middle_track_notches_match_v0312() {
    notches_match_reference(Slot::Active);
}

#[test]
fn colwheel_ancestor_tracks_notches_match_v0312() {
    for slot in [Slot::Parent, Slot::Grandparent, Slot::GreatGrandparent] {
        notches_match_reference(slot);
    }
}

#[test]
fn colwheel_touchpad_pixels_pass_through_on_every_track() {
    let mut cx = host::context();
    let handle = mounted(&mut cx);
    let mut expected = [0.; 5];
    for slot in [
        Slot::Parent,
        Slot::Grandparent,
        Slot::GreatGrandparent,
        Slot::Active,
        Slot::Child,
    ] {
        let at = inside(&mut cx, handle, slot);
        for (y, phase) in [
            (0., TouchPhase::Started),
            (-11.5, TouchPhase::Moved),
            (-7.25, TouchPhase::Moved),
            (2.5, TouchPhase::Moved),
            (0., TouchPhase::Ended),
        ] {
            send(
                &mut cx,
                handle,
                at,
                ScrollDelta::Pixels(point(px(0.), px(y))),
                phase,
            );
            expected[slot.index()] -= y;
            assert_eq!(scrolls(&mut cx, handle), expected, "{slot:?} pixel {y}");
        }
    }
}

#[test]
fn colwheel_pixels_outside_a_stroke_move_each_track_one_to_one() {
    let mut cx = host::context();
    let handle = mounted(&mut cx);
    let mut expected = [0.; 5];
    for slot in [
        Slot::Parent,
        Slot::Grandparent,
        Slot::GreatGrandparent,
        Slot::Active,
        Slot::Child,
    ] {
        let at = inside(&mut cx, handle, slot);
        for y in [-11.5, -7.25, 2.5] {
            send(
                &mut cx,
                handle,
                at,
                ScrollDelta::Pixels(point(px(0.), px(y))),
                TouchPhase::Moved,
            );
            expected[slot.index()] -= y;
            assert_eq!(
                scrolls(&mut cx, handle),
                expected,
                "{slot:?} pixel {y} outside a stroke"
            );
        }
    }
}

fn painted(cx: &mut TestAppContext, handle: WindowHandle<Shell>, slot: Slot) -> f32 {
    let mut painted = 0.;
    host::change(cx, handle, |shell, _, _| {
        let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
        let paint = columns.painters.get(slot).unwrap().borrow();
        painted = paint.drawn.as_ref().expect("the track is mounted").scroll;
    });
    painted
}

#[test]
fn colwheel_notch_paint_still_glides_one_frame_after_the_notch() {
    for slot in [
        Slot::Parent,
        Slot::Grandparent,
        Slot::GreatGrandparent,
        Slot::Active,
        Slot::Child,
    ] {
        let mut cx = host::context();
        let handle = mounted(&mut cx);
        let at = inside(&mut cx, handle, slot);
        send(
            &mut cx,
            handle,
            at,
            ScrollDelta::Lines(point(0., -3.)),
            TouchPhase::Moved,
        );
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(16));
        host::frame(&mut cx, handle);
        let painted = painted(&mut cx, handle, slot);
        assert!(
            painted > 0. && painted < NOTCH_PX,
            "{slot:?}: one frame after a notch paints {painted}, not strictly between 0 and {NOTCH_PX}"
        );
    }
}

#[test]
fn colwheel_notch_paints_its_target_on_the_first_frame_under_reduced_motion() {
    let mut cx = host::context();
    cx.update(|cx| cx.set_reduce_motion(true));
    let handle = mounted(&mut cx);
    let at = inside(&mut cx, handle, Slot::Active);
    send(
        &mut cx,
        handle,
        at,
        ScrollDelta::Lines(point(0., -3.)),
        TouchPhase::Moved,
    );
    host::frame(&mut cx, handle);
    assert_eq!(painted(&mut cx, handle, Slot::Active), NOTCH_PX);
}

#[test]
fn colwheel_list_and_grid_notch_distances_remain_byte_identical() {
    for view in [View::List, View::Grid] {
        let mut cx = host::context();
        let handle = mounted(&mut cx);
        host::change(&mut cx, handle, |shell, window, cx| {
            shell.set_view(view, window, cx)
        });
        host::frame(&mut cx, handle);
        let mut at = None;
        host::change(&mut cx, handle, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            at = Some(if view == View::Grid {
                host.grid.as_ref().unwrap().state.borrow().bounds.center()
            } else {
                host.list.borrow().drawn.bounds.center()
            });
        });
        for count in 1..=4 {
            send(
                &mut cx,
                handle,
                at.unwrap(),
                ScrollDelta::Lines(point(0., -3.)),
                TouchPhase::Moved,
            );
            host::change(&mut cx, handle, |shell, _, _| {
                let host = shell.panes[0].as_ref().unwrap();
                let actual = if view == View::Grid {
                    host.grid.as_ref().unwrap().view.scroll
                } else {
                    host.pane.scroll_y
                };
                assert_eq!(
                    actual.to_bits(),
                    (count as f32 * NOTCH_PX).to_bits(),
                    "{view:?}"
                );
            });
        }
    }
}

#[path = "columns_wheel_fixlist_tests.rs"]
mod fixlist;
