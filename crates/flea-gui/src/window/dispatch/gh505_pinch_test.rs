/// A touchpad pinch scales from the zoom it starts at, keeps the picture point under the fingers in
/// place, holds at 400 %, turns back at once, and a pinch in to the fit draws the picture as it opened.
fn gh505_pinch_event(at: gpui::Point<Pixels>, phase: gpui::TouchPhase, delta: f32) -> gpui::PinchEvent {
    gpui::PinchEvent {
        position: at,
        delta,
        modifiers: gpui::Modifiers::default(),
        phase,
    }
}

#[gpui::test]
fn gh505_pinch_zooms_the_picture_about_the_fingers(cx: &mut TestAppContext) {
    use gpui::TouchPhase::{Ended, Moved, Started};
    let _watchdog = watchdog(300);
    cx.executor().allow_parking();
    let root = root();
    let (shell, cx, mut requests) = opened_at(cx, &root.0);
    land(&shell, cx, &root.0.to_string_lossy(), image_listing(&root.0), 1);
    open_picture(&shell, cx, &mut requests, 1, 300, 600);
    let look = look(&shell, cx);
    raster_other_than(cx, &look, None);
    let initial = picture_rect(cx);
    let w = |r: Bounds<Pixels>| f64::from(f32::from(r.size.width));
    // A quarter of the way into the picture on both axes, so a zoom about the centre would move it.
    let at = initial.origin + gpui::point(initial.size.width / 4., initial.size.height / 4.);
    cx.simulate_event(gh505_pinch_event(at, Started, 0.));
    cx.simulate_event(gh505_pinch_event(at, Moved, 1.5));
    draw(cx);
    let rect = picture_rect(cx);
    assert!((w(rect) / w(initial) - 2.5).abs() < 0.01, "a 2.5x pinch zooms to 250 %");
    let view = look.read_with(cx, |look, _| look.scroll.view.expect("a zoomed picture scrolls"));
    let fraction = |p: Pixels, o: Pixels, s: Pixels| f32::from(p - o) / f32::from(s);
    let mut checked = 0;
    if view.content_w > w(view.bounds) + 1. {
        let fx = fraction(at.x, rect.origin.x, rect.size.width);
        assert!((fx - 0.25).abs() < 0.01, "the point under the fingers stays put across, at {fx}");
        checked += 1;
    }
    if view.content_h > f64::from(f32::from(view.bounds.size.height)) + 1. {
        let fy = fraction(at.y, rect.origin.y, rect.size.height);
        assert!((fy - 0.25).abs() < 0.01, "the point under the fingers stays put down, at {fy}");
        checked += 1;
    }
    assert!(checked > 0, "at 250 % the picture overflows on at least one axis");
    cx.simulate_event(gh505_pinch_event(at, Moved, 5.));
    draw(cx);
    assert!((w(picture_rect(cx)) / w(initial) - 4.).abs() < 0.01, "the pinch holds at 400 %");
    cx.simulate_event(gh505_pinch_event(at, Moved, -0.5));
    draw(cx);
    let back = w(picture_rect(cx)) / w(initial);
    assert!(back < 3.9, "pinching back from the hold zooms out at once, at {back}");
    cx.simulate_event(gh505_pinch_event(at, Ended, 0.));
    // A second pinch starts from where the first ended, and pinching right in returns to the fit.
    cx.simulate_event(gh505_pinch_event(at, Started, 0.));
    cx.simulate_event(gh505_pinch_event(at, Moved, -0.95));
    cx.simulate_event(gh505_pinch_event(at, Ended, 0.));
    draw(cx);
    assert_eq!(picture_rect(cx), initial, "a pinch to the fit draws the picture as it opened");
    assert!(
        look.read_with(cx, |look, _| look.scroll.view.is_none()),
        "at the fit nothing scrolls"
    );
    close(&shell, cx);
}
