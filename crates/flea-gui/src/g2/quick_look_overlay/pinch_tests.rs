//! The pinch's zoom arithmetic: the fingers' scale times the start, held to the ladder, turning at once at an end.
use super::scaled;
use flea::ui_logic_preview_core::keys::{ZOOM_MAX, ZOOM_MIN};

#[test]
fn a_pinch_scales_the_zoom_it_started_from() {
    let mut start = 1.5;
    assert_eq!(scaled(&mut start, 2.), 3.);
    assert_eq!(start, 1.5, "inside the ladder the start stays");
}

#[test]
fn a_pinch_holds_at_the_ladder_ends() {
    let mut start = 1.;
    assert_eq!(scaled(&mut start, 9.), ZOOM_MAX);
    let mut start = 2.;
    assert_eq!(scaled(&mut start, 0.1), ZOOM_MIN);
}

#[test]
fn pinching_back_past_an_end_turns_around_at_once() {
    let mut start = 1.;
    assert_eq!(scaled(&mut start, 6.), ZOOM_MAX);
    // Without the moved start this would read 5.5 and stay held at 4 until the fingers came back under 4.
    let next = scaled(&mut start, 5.5);
    assert!(next < ZOOM_MAX, "the first step back zooms out, got {next}");
    assert!((next - ZOOM_MAX * 5.5 / 6.).abs() < 1e-9);
}

#[test]
fn a_collapsed_scale_holds_at_the_fit_without_dividing_by_zero() {
    let mut start = 2.;
    assert_eq!(scaled(&mut start, 0.), ZOOM_MIN);
    assert!(start.is_finite());
}
