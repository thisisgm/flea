//! EY-288 (PB-DRAG-1): a row lift resting in a List bottom band scrolls about a row each 100 ms and stops off the band.
use super::mounted_tests::{frame, open_with, Root};
use crate::list::bench;
use gpui::{point, px, Modifiers, MouseButton, VisualTestContext};
use std::time::Duration;

#[test]
fn ey288_a_row_lift_resting_in_the_bottom_band_scrolls_about_a_row_each_100ms() {
    let root = Root::new("edge");
    let mut cx = bench::real_cx();
    let (handle, _lines) = open_with(&mut cx, root.src(), 200);
    frame(&mut cx, handle);
    let (from, band, row) = handle
        .update(&mut cx, |shell, _, _| {
            let frame = shell.panes[0].as_ref().unwrap().input.frame.unwrap();
            let x = frame.bounds.origin.x + px(40.);
            (
                point(x, frame.bounds.top() + px(frame.row * 1.5)),
                point(x, frame.bounds.bottom() - px(10.)),
                frame.row,
            )
        })
        .unwrap();
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    vcx.simulate_mouse_move(from, None, Modifiers::default());
    vcx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    vcx.simulate_mouse_move(
        point(from.x + px(20.), from.y),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    vcx.simulate_mouse_move(band, Some(MouseButton::Left), Modifiers::default());
    vcx.run_until_parked();
    // The edge step is paced by presented frames, so the test presents one every 16 ms of clock.
    let present = |vcx: &mut VisualTestContext, millis: u64| {
        for _ in 0..millis / 16 {
            vcx.executor().advance_clock(Duration::from_millis(16));
            frame(vcx, handle);
        }
        vcx.run_until_parked();
    };
    let scroll = |vcx: &mut VisualTestContext| {
        handle
            .read_with(vcx, |shell, _| {
                shell.panes[0].as_ref().unwrap().pane.scroll_y
            })
            .unwrap()
    };
    assert_eq!(scroll(&mut vcx), 0.);
    present(&mut vcx, 112);
    let first = scroll(&mut vcx);
    assert!(
        first >= row * 0.5 && first <= row * 1.5,
        "about one row after 100 ms: {first}"
    );
    present(&mut vcx, 208);
    let rested = scroll(&mut vcx);
    assert!(
        rested > first,
        "it keeps scrolling while it rests: {rested}"
    );
    let middle = point(band.x, band.y - px(200.));
    vcx.simulate_mouse_move(middle, Some(MouseButton::Left), Modifiers::default());
    vcx.run_until_parked();
    let left = scroll(&mut vcx);
    present(&mut vcx, 304);
    assert_eq!(scroll(&mut vcx), left, "leaving the band stops it");
    vcx.simulate_mouse_up(middle, MouseButton::Left, Modifiers::default());
}
