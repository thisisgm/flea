// tests/js/scroll.js value checks for the wheel, the bounds and the overlay scroller, on the window's own delta shape.
use super::scroll::*;

#[test]
fn a_notch_and_a_stroke_move_as_scroll_js_does() {
    // one notch down moves lines times notch pixels times the multiplier (the window's Lines already carry 3 lines)
    assert_eq!(distance(Delta::Lines(-3.0)), -288.0);
    assert_eq!(distance(Delta::Lines(3.0)), 288.0);
    assert_eq!(distance(Delta::Lines(-6.0)), -576.0);
    // a stroke of compositor pixels (Qt pixelDelta, before any GPUI factor) moves at the 2.5 gain and keeps its fraction
    assert_eq!(distance(Delta::Pixels(-10.0)), -25.0);
    assert_eq!(distance(Delta::Pixels(-1.0)), -2.5);
    assert_eq!(distance(Delta::Pixels(0.0)), 0.0);
}

#[test]
fn writes_stay_inside_the_bounds() {
    assert_eq!(bounded(-50.0, 1000.0, 400.0), 0.0);
    assert_eq!(bounded(5000.0, 1000.0, 400.0), 600.0);
    assert_eq!(bounded(250.0, 1000.0, 400.0), 250.0);
    // a content shorter than the view pins to the origin
    assert_eq!(bounded(100.0, 300.0, 400.0), 0.0);
    assert_eq!(scrolled(0.0, Delta::Lines(-3.0), 1000.0, 400.0), 288.0);
    assert_eq!(scrolled(500.0, Delta::Lines(-3.0), 1000.0, 400.0), 600.0);
}

#[test]
fn the_scroller_describes_the_viewport() {
    assert_eq!(range(400.0, 400.0), 0.0);
    assert_eq!(range(1.0, 400.0), 0.0);
    assert_eq!(range(1000.0, 400.0), 600.0);
    assert!(!overflows(400.2, 400.0) && overflows(401.0, 400.0));
    assert_eq!(handle_length(500.0, 1000.0, 400.0), 200.0);
    assert_eq!(travel(500.0, 1000.0, 400.0), 300.0);
    assert_eq!(travel(20.0, 1000.0, 400.0), 0.0);
    assert_eq!(handle_length(500.0, 3_700_000.0, 500.0), 24.0);
    assert_eq!(handle_offset(0.0, 1000.0, 400.0, 500.0), 0.0);
    assert_eq!(handle_offset(600.0, 1000.0, 400.0, 500.0), 300.0);
    assert_eq!(handle_offset(-100.0, 1000.0, 400.0, 500.0), 0.0);
    assert_eq!(handle_offset(900.0, 1000.0, 400.0, 500.0), 300.0);
    assert_eq!(handle_offset(49800.0, 100_000.0, 400.0, 400.0).round(), 188.0);
    assert_eq!(handle_offset(99600.0, 100_000.0, 400.0, 400.0), 376.0);
    assert_eq!(position_for_handle(900.0, 100_000.0, 400.0, 400.0), 99600.0);
    assert_eq!(position_for_handle(10.0, 1000.0, 400.0, 20.0), 0.0);
}

#[test]
fn finders_reveal_and_the_track_press() {
    assert!(!revealed(false, true, true, true));
    assert!(!revealed(true, false, false, false));
    assert!(revealed(true, true, false, false) && revealed(true, false, true, false) && revealed(true, false, false, true));
    assert_eq!(jump_offset(250.0, 100.0, 400.0), 200.0);
    assert_eq!(jump_offset(20.0, 100.0, 400.0), 0.0);
    assert_eq!(jump_offset(495.0, 100.0, 400.0), 400.0);
    assert_eq!(jump_offset(250.0, 100.0, 0.0), 0.0);
    assert_eq!(knob_radius(0.0, 6.0, 100.0), 0.0);
    assert_eq!(knob_radius(8.0, 6.0, 100.0), 3.0);
    assert_eq!(knob_radius(8.0, 10.0, 100.0), 5.0);
    assert_eq!(knob_radius(3.0, 10.0, 100.0), 3.0);
}

#[test]
fn only_the_rows_in_view_draw() {
    // 12 px margin, 22 px rows, a 546 px view over 81 rows
    assert_eq!(rows_in_view(0.0, 12.0, 22.0, 546.0, 81), (0, 25));
    assert_eq!(rows_in_view(660.0, 12.0, 22.0, 546.0, 81), (29, 55));
    assert_eq!(rows_in_view(1260.0, 12.0, 22.0, 546.0, 81), (56, 81));
    assert_eq!(rows_in_view(0.0, 0.0, 20.0, 0.0, 0), (0, 0));
}
