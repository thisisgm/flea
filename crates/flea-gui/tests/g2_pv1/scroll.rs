//! The diff and the text card scroll by the wheel and the touchpad inside the card, and the listing behind hears none of it.
use super::boards::rect;
use super::scenes::*;
use super::*;
use flea::ui_logic_preview_core::compare;

fn english() -> flea::i18n::I18n {
    crate::test_catalogs::english()
}

/// An 81-row patch: the hunk header and 80 context lines, three screens of a 574 px card.
fn long_patch(i18n: &flea::i18n::I18n) -> Subject {
    let mut patch = String::from("--- /w/a.txt\n+++ /w/b.txt\n@@ -1,80 +1,80 @@\n");
    for n in 1..=80 {
        patch.push_str(&format!(" line {n} of the long patch\n"));
    }
    let mut subject = compare_diff(i18n);
    if let Body::Diff { rows, .. } = &mut subject.body {
        *rows = compare::rows(&patch);
        assert_eq!(rows.len(), 81);
    }
    subject
}

/// A 100-line text file in the text card.
fn long_text(i18n: &flea::i18n::I18n) -> Subject {
    let mut subject = ql_text(i18n);
    if let Body::Text { lines, count } = &mut subject.body {
        *lines = (1..=100).map(|n| format!("text line {n}")).collect();
        *count = i18n.trn("preview.lines", 100);
    }
    subject
}

fn wheel(cx: &mut VisualTestContext, at: Point<Pixels>, delta: ScrollDelta) {
    cx.simulate_event(ScrollWheelEvent {
        position: at,
        delta,
        modifiers: Modifiers::none(),
        touch_phase: TouchPhase::Moved,
    });
    cx.run_until_parked();
}

fn drawn(cx: &mut VisualTestContext, selector: &'static str) -> bool {
    cx.debug_bounds(selector).is_some()
}

#[gpui::test]
fn compare_wheel_scrolls_a_long_patch_and_the_listing_stays_still(cx: &mut TestAppContext) {
    let (cx, seen, _, _) = host(cx, long_patch(&english()), Arm::Fix);
    let card = cx.debug_bounds("pv1-card").unwrap();
    assert!(drawn(cx, "pv1-diff-24") && !drawn(cx, "pv1-diff-25"));
    // the lane is space at rest: no knob until the view is used
    assert!(!drawn(cx, "pv1-scroll-knob"));
    // a 264 px stroke reaches GPUI as 792 px (its Wayland 3.0) and moves the view 660 px, Scroll.js's 2.5 gain
    wheel(
        cx,
        card.center(),
        ScrollDelta::Pixels(point(px(0.), px(-792.))),
    );
    assert!(drawn(cx, "pv1-diff-40") && !drawn(cx, "pv1-diff-0"));
    assert_eq!(rect(cx, "pv1-diff-30").1, 102. + 30. * 22. - 660.);
    assert!(drawn(cx, "pv1-scroll-knob"));
    // a wheel notch is 3 lines of 24 px times 4; far past the end it rests on the last row and the bottom margin
    wheel(cx, card.center(), ScrollDelta::Lines(point(0., -300.)));
    let (_, y, _, h) = rect(cx, "pv1-diff-80");
    assert_eq!(y + h, 63. + 574. - 1. - 12.);
    wheel(cx, card.center(), ScrollDelta::Lines(point(0., 300.)));
    assert_eq!(rect(cx, "pv1-diff-0").1, 102.);
    assert_eq!(*seen.borrow(), Seen::default());
    // Finder's hold: the knob leaves a second after the view stops
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(1001));
    cx.run_until_parked();
    assert!(!drawn(cx, "pv1-scroll-knob"));
}

#[gpui::test]
fn compare_scrolls_sideways_to_its_longest_line(cx: &mut TestAppContext) {
    let i18n = english();
    let mut subject = compare_diff(&i18n);
    if let Body::Diff { rows, .. } = &mut subject.body {
        rows[3].text = "x".repeat(400);
    }
    let (cx, seen, _, _) = host(cx, subject, Arm::Fix);
    let card = cx.debug_bounds("pv1-card").unwrap();
    let before = rect(cx, "pv1-diff-3").0;
    wheel(
        cx,
        card.center(),
        ScrollDelta::Pixels(point(px(-120.), px(0.))),
    );
    assert_eq!(rect(cx, "pv1-diff-3").0, before - 100.);
    assert_eq!(*seen.borrow(), Seen::default());
}

#[gpui::test]
fn sol_preview_remainder_ey584_shared_corner_reveals_both_overflow_bars(cx: &mut TestAppContext) {
    let mut subject = long_patch(&english());
    if let Body::Diff { rows, .. } = &mut subject.body {
        rows[3].text = "x".repeat(400);
    }
    let (cx, seen, _, _) = host(cx, subject, Arm::Fix);
    let card = cx.debug_bounds("pv1-card").unwrap();
    let corner = point(card.right() - px(6.), card.bottom() - px(6.));
    assert!(!drawn(cx, "pv1-scroll-knob-x"));
    cx.simulate_mouse_move(corner, None, Modifiers::none());
    cx.run_until_parked();
    assert!(
        drawn(cx, "pv1-scroll-knob-x"),
        "Compare's horizontal thumb must remain visible at the shared corner"
    );
    assert!(drawn(cx, "pv1-scroll-knob"));
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(1001));
    cx.run_until_parked();
    assert!(drawn(cx, "pv1-scroll-knob-x") && drawn(cx, "pv1-scroll-knob"));
    wheel(cx, corner, ScrollDelta::Lines(point(-300., -300.)));
    let horizontal = cx.debug_bounds("pv1-scroll-knob-x").unwrap();
    let vertical = cx.debug_bounds("pv1-scroll-knob").unwrap();
    assert_eq!(horizontal.size.height, px(10.));
    assert_eq!(vertical.size.width, px(10.));
    assert!(horizontal.right() <= card.right() - px(14.));
    assert!(vertical.bottom() <= card.bottom() - px(14.));
    cx.simulate_mouse_move(card.center(), None, Modifiers::none());
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(1001));
    cx.run_until_parked();
    assert!(!drawn(cx, "pv1-scroll-knob-x") && !drawn(cx, "pv1-scroll-knob"));
    assert_eq!(*seen.borrow(), Seen::default());
}

#[gpui::test]
fn a_short_patch_takes_the_wheel_and_moves_nothing(cx: &mut TestAppContext) {
    let (cx, seen, _, _) = host(cx, compare_diff(&english()), Arm::Fix);
    let card = cx.debug_bounds("pv1-card").unwrap();
    wheel(cx, card.center(), ScrollDelta::Lines(point(0., -3.)));
    assert_eq!(rect(cx, "pv1-diff-0").1, 102.);
    assert!(!drawn(cx, "pv1-scroll-knob"));
    assert_eq!(*seen.borrow(), Seen::default());
}

#[gpui::test]
fn the_text_card_scrolls_its_lines(cx: &mut TestAppContext) {
    let (cx, seen, _, _) = host(cx, long_text(&english()), Arm::Fix);
    let card = cx.debug_bounds("pv1-card").unwrap();
    assert!(!drawn(cx, "pv1-line-40"));
    wheel(cx, card.center(), ScrollDelta::Lines(point(0., -3.)));
    // one notch is 288 px
    assert_eq!(rect(cx, "pv1-line-20").1, 66. + 20. * 20. - 288.);
    assert!(drawn(cx, "pv1-line-40") && drawn(cx, "pv1-scroll-knob"));
    wheel(cx, card.center(), ScrollDelta::Lines(point(0., -300.)));
    // the last line rests 14 px over the card's inner bottom, PreviewText's margin
    let (_, y, _, h) = rect(cx, "pv1-line-99");
    assert_eq!(y + h, 24. + 652. - 1. - 14.);
    assert_eq!(*seen.borrow(), Seen::default());
}

#[gpui::test]
fn a_wheel_over_the_bar_scrolls_nothing(cx: &mut TestAppContext) {
    let (cx, seen, _, _) = host(cx, long_patch(&english()), Arm::Fix);
    let bar = cx.debug_bounds("pv1-bar").unwrap();
    wheel(cx, bar.center(), ScrollDelta::Lines(point(0., -3.)));
    assert_eq!(rect(cx, "pv1-diff-0").1, 102.);
    assert_eq!(*seen.borrow(), Seen::default());
}

#[gpui::test]
fn the_lane_widens_on_hover_jumps_on_a_press_and_drags(cx: &mut TestAppContext) {
    let (cx, seen, _, _) = host(cx, long_patch(&english()), Arm::Fix);
    let none = Modifiers::none();
    // the diff viewport is 100,90 900x546, so its vertical lane is x 986 to 1000
    let lane = |y: f32| point(px(993.), px(y));
    cx.simulate_mouse_move(lane(300.), None, none);
    cx.run_until_parked();
    // over the lane the knob widens to 10 px, 2 px in from the edge, over the 5% track
    let (x, y, w, _) = rect(cx, "pv1-scroll-knob");
    assert_eq!((x, y, w), (988., 90., 10.));
    // a press under the knob centres it on the pointer, clamped at the end of its travel
    cx.simulate_mouse_down(lane(600.), MouseButton::Left, none);
    cx.run_until_parked();
    let (_, y, _, h) = rect(cx, "pv1-diff-80");
    assert_eq!(y + h, 63. + 574. - 1. - 12.);
    // the held knob follows the pointer back to the top
    cx.simulate_mouse_move(lane(100.), Some(MouseButton::Left), none);
    cx.run_until_parked();
    assert_eq!(rect(cx, "pv1-diff-0").1, 102.);
    cx.simulate_mouse_up(lane(100.), MouseButton::Left, none);
    cx.simulate_mouse_move(point(px(500.), px(300.)), None, none);
    cx.run_until_parked();
    // off the lane the knob narrows to 6 px and leaves a second after the drag
    assert_eq!(rect(cx, "pv1-scroll-knob").2, 6.);
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(1001));
    cx.run_until_parked();
    assert!(!drawn(cx, "pv1-scroll-knob"));
    assert_eq!(*seen.borrow(), Seen::default());
}

#[gpui::test]
fn a_ten_pixel_stroke_moves_twenty_five(cx: &mut TestAppContext) {
    let (cx, _, _, _) = host(cx, long_patch(&english()), Arm::Fix);
    let card = cx.debug_bounds("pv1-card").unwrap();
    // the compositor's 10 px arrives as 30 (gpui-pre-linux wayland/client.rs:2774), and Qt's pixelDelta x 2.5 is 25
    wheel(
        cx,
        card.center(),
        ScrollDelta::Pixels(point(px(0.), px(-30.))),
    );
    assert_eq!(rect(cx, "pv1-diff-0").1, 102. - 25.);
}

#[gpui::test]
fn a_release_outside_the_window_ends_the_drag(cx: &mut TestAppContext) {
    let (cx, seen, _, _) = host(cx, long_patch(&english()), Arm::Fix);
    let none = Modifiers::none();
    let lane = |y: f32| point(px(993.), px(y));
    cx.simulate_mouse_move(lane(150.), None, none);
    cx.simulate_mouse_down(lane(150.), MouseButton::Left, none);
    cx.simulate_mouse_move(lane(400.), Some(MouseButton::Left), none);
    // the window is 1100x700, so this release lands below it
    cx.simulate_mouse_up(lane(760.), MouseButton::Left, none);
    cx.run_until_parked();
    let held = rect(cx, "pv1-diff-40").1;
    cx.simulate_mouse_move(point(px(500.), px(300.)), None, none);
    cx.simulate_mouse_move(lane(200.), None, none);
    cx.run_until_parked();
    assert_eq!(
        rect(cx, "pv1-diff-40").1,
        held,
        "the view moved under a pointer with no button down"
    );
    cx.simulate_mouse_move(point(px(500.), px(300.)), None, none);
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(1001));
    cx.run_until_parked();
    assert!(!drawn(cx, "pv1-scroll-knob"));
    assert!(
        drawn(cx, "pv1-card"),
        "a drag released outside the window leaves the card open"
    );
    assert_eq!(*seen.borrow(), Seen::default());
}

#[gpui::test]
fn a_move_with_no_button_ends_a_drag_whose_release_was_lost(cx: &mut TestAppContext) {
    let (cx, _, _, _) = host(cx, long_patch(&english()), Arm::Fix);
    let none = Modifiers::none();
    let lane = |y: f32| point(px(993.), px(y));
    cx.simulate_mouse_move(lane(150.), None, none);
    cx.simulate_mouse_down(lane(150.), MouseButton::Left, none);
    cx.run_until_parked();
    let held = rect(cx, "pv1-diff-0").1;
    cx.simulate_mouse_move(lane(400.), None, none);
    cx.run_until_parked();
    assert_eq!(rect(cx, "pv1-diff-0").1, held);
}

#[gpui::test]
fn a_drag_past_the_bottom_edge_reaches_the_end(cx: &mut TestAppContext) {
    let (cx, _, _, _) = host(cx, long_patch(&english()), Arm::Fix);
    let none = Modifiers::none();
    let lane = |y: f32| point(px(993.), px(y));
    cx.simulate_mouse_move(lane(150.), None, none);
    cx.simulate_mouse_down(lane(150.), MouseButton::Left, none);
    cx.simulate_mouse_move(lane(760.), Some(MouseButton::Left), none);
    cx.run_until_parked();
    let (_, y, _, h) = rect(cx, "pv1-diff-80");
    assert_eq!(y + h, 63. + 574. - 1. - 12.);
}

#[gpui::test]
fn a_release_outside_the_window_ends_the_drag_with_no_later_move(cx: &mut TestAppContext) {
    let (cx, _, _, _) = host(cx, long_patch(&english()), Arm::Fix);
    let none = Modifiers::none();
    let lane = |y: f32| point(px(993.), px(y));
    cx.simulate_mouse_move(lane(150.), None, none);
    cx.simulate_mouse_down(lane(150.), MouseButton::Left, none);
    cx.simulate_mouse_move(lane(400.), Some(MouseButton::Left), none);
    cx.simulate_mouse_move(lane(760.), Some(MouseButton::Left), none);
    // after a release below the window Wayland sends leave and no motion, so only the release can end the drag
    cx.simulate_mouse_up(lane(760.), MouseButton::Left, none);
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(1001));
    cx.run_until_parked();
    assert!(
        !drawn(cx, "pv1-scroll-knob"),
        "the knob is still held after a release outside the window"
    );
}
