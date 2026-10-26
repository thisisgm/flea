//! Presses that must never lift files: under the slop, blank space, a stale earlier press, the middle button and the scroll lane.
use super::mounted_tests::{centre, drag, dragging, drain, frame, open, open_with, Root};
use crate::list::bench;
use gpui::{point, px, Modifiers, MouseButton, VisualTestContext};

#[test]
fn m9_dg2_mounted_travel_under_the_slop_sends_nothing() {
    let root = Root::new("tap");
    let mut cx = bench::real_cx();
    let (handle, mut lines) = open(&mut cx, root.src());
    frame(&mut cx, handle);
    let file = centre(&mut cx, handle, 1);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    drag(&mut vcx, file, point(file.x + px(4.), file.y + px(3.)));
    vcx.run_until_parked();
    assert!(drain(&mut lines)
        .iter()
        .all(|line| !line.contains("transfer") && !line.contains("collisions")));
}

#[test]
fn m9_dg2_mounted_a_blank_press_never_lifts_an_earlier_press() {
    let root = Root::new("stale");
    let mut cx = bench::real_cx();
    let (handle, mut lines) = open(&mut cx, root.src());
    frame(&mut cx, handle);
    let file = centre(&mut cx, handle, 1);
    let folder = centre(&mut cx, handle, 0);
    let blank = point(file.x, file.y + px(200.));
    // A press a drop elsewhere left behind, as a drop on the other pane leaves it.
    handle
        .update(&mut cx, |shell, _, _| {
            let host = shell.panes[0].as_mut().unwrap();
            host.input.press = Some(crate::input::Press {
                row: 1,
                listing: host.pane.held.listing,
                name: Some("b.txt".into()),
                event: gpui::MouseDownEvent {
                    position: file,
                    button: MouseButton::Left,
                    modifiers: Modifiers::default(),
                    click_count: 1,
                    first_mouse: false,
                },
                dragged: true,
            });
        })
        .unwrap();
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    drag(&mut vcx, blank, folder);
    vcx.run_until_parked();
    assert!(drain(&mut lines)
        .iter()
        .all(|line| !line.contains("transfer") && !line.contains("collisions")));
}

#[test]
fn m9_dg2_mounted_a_blank_press_drags_a_marquee_not_files() {
    let root = Root::new("blank");
    let mut cx = bench::real_cx();
    let (handle, mut lines) = open(&mut cx, root.src());
    frame(&mut cx, handle);
    let folder = centre(&mut cx, handle, 0);
    let blank = point(folder.x, folder.y + px(240.));
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    vcx.simulate_mouse_move(blank, None, Modifiers::default());
    vcx.simulate_mouse_down(blank, MouseButton::Left, Modifiers::default());
    vcx.simulate_mouse_move(folder, Some(MouseButton::Left), Modifiers::default());
    assert!(!dragging(&mut vcx), "a blank press starts no file drag");
    vcx.simulate_mouse_up(folder, MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    assert!(drain(&mut lines)
        .iter()
        .all(|line| !line.contains("transfer") && !line.contains("collisions")));
}

#[test]
fn m9_dg2_mounted_a_middle_button_row_drag_lifts_nothing() {
    let root = Root::new("middle");
    let mut cx = bench::real_cx();
    let (handle, mut lines) = open(&mut cx, root.src());
    frame(&mut cx, handle);
    let file = centre(&mut cx, handle, 1);
    let folder = centre(&mut cx, handle, 0);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    vcx.simulate_mouse_move(file, None, Modifiers::default());
    vcx.simulate_mouse_down(file, MouseButton::Middle, Modifiers::default());
    vcx.simulate_mouse_move(folder, Some(MouseButton::Middle), Modifiers::default());
    assert!(!dragging(&mut vcx), "only a left press lifts");
    vcx.simulate_mouse_up(folder, MouseButton::Middle, Modifiers::default());
    vcx.run_until_parked();
    assert!(drain(&mut lines)
        .iter()
        .all(|line| !line.contains("transfer") && !line.contains("collisions")));
}

#[test]
fn m9_dg2_mounted_a_scroll_lane_drag_lifts_nothing() {
    let root = Root::new("lane");
    let mut cx = bench::real_cx();
    let (handle, mut lines) = open_with(&mut cx, root.src(), 300);
    frame(&mut cx, handle);
    let folder = centre(&mut cx, handle, 0);
    let lane = handle
        .update(&mut cx, |shell, _, _| {
            let frame = shell.panes[0]
                .as_ref()
                .unwrap()
                .input
                .frame
                .expect("a painted pane");
            assert!(frame.max_scroll() > 0.5, "the fixture scrolls");
            let y = frame.bounds.center().y;
            (0..400)
                .map(|dx| point(frame.bounds.right() - px(1. + dx as f32), y))
                .find(|at| frame.in_lane(*at))
                .expect("a scroll lane")
        })
        .unwrap();
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    vcx.simulate_mouse_move(lane, None, Modifiers::default());
    vcx.simulate_mouse_down(lane, MouseButton::Left, Modifiers::default());
    vcx.simulate_mouse_move(
        point(lane.x, lane.y + px(80.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    assert!(!dragging(&mut vcx), "a lane press scrolls, it never lifts");
    vcx.simulate_mouse_move(folder, Some(MouseButton::Left), Modifiers::default());
    vcx.simulate_mouse_up(folder, MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    assert!(drain(&mut lines)
        .iter()
        .all(|line| !line.contains("transfer") && !line.contains("collisions")));
}
