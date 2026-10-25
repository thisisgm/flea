use super::*;
use crate::input::scroll;
use gpui::{point, Entity, Modifiers, MouseButton, ScrollDelta, ScrollWheelEvent, TouchPhase};
use std::time::Duration;

fn mounted(cx: &mut TestAppContext) -> (Entity<Shell>, &mut gpui::VisualTestContext) {
    let (backend, _) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let shell = shell_with(backend, cx);
        window.focus(&shell.focus, cx);
        shell
    });
    cx.run_until_parked();
    (shell, cx)
}

fn inside(shell: &Entity<Shell>, cx: &mut gpui::VisualTestContext) -> gpui::Point<gpui::Pixels> {
    let frame = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().input.frame.unwrap()
    });
    point(
        frame.bounds.left() + px(60.),
        frame.bounds.top() + px(frame.row * 2.5),
    )
}

fn wheel(at: gpui::Point<gpui::Pixels>, delta: ScrollDelta, phase: TouchPhase) -> ScrollWheelEvent {
    ScrollWheelEvent {
        position: at,
        delta,
        touch_phase: phase,
        ..Default::default()
    }
}

fn pixels(y: f32) -> ScrollDelta {
    ScrollDelta::Pixels(point(px(0.), px(y)))
}

/// Steps the motion as presented frames would, `ms` apart, without waiting on a real frame clock.
fn frames(shell: &Entity<Shell>, cx: &mut gpui::VisualTestContext, count: usize, ms: u64) {
    for _ in 0..count {
        cx.executor().advance_clock(Duration::from_millis(ms));
        shell.update(cx, |shell, cx| {
            let now = cx.background_executor().now();
            let host = shell.panes[0].as_mut().unwrap();
            host.input.step_motion(now, &mut host.pane, false);
        });
    }
}

#[gpui::test]
fn g1_05_a_notch_writes_288_px_at_once_and_draws_an_outcubic_glide(cx: &mut TestAppContext) {
    let (shell, cx) = mounted(cx);
    let at = inside(&shell, cx);
    cx.simulate_event(wheel(
        at,
        ScrollDelta::Lines(point(0., -3.)),
        TouchPhase::Moved,
    ));
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(
            host.pane.scroll_y, 288.,
            "three lines are one notch of 3 x 24 x 4"
        );
        assert!(host.input.glide.is_some());
        assert!(
            host.input.drawn(host.pane.scroll_y) < 288.,
            "the rows draw from where they stood"
        );
    });
    frames(&shell, cx, 1, 90);
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        let expected = 288. * scroll::out_cubic(0.5) as f32;
        assert!((host.input.drawn(host.pane.scroll_y) - expected).abs() < 1.);
    });
    frames(&shell, cx, 1, 100);
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(host.input.glide.is_none() && !host.input.moving());
        assert_eq!(host.input.drawn(host.pane.scroll_y), 288.);
    });
}

#[gpui::test]
fn g1_05_a_trackpoint_frame_outside_a_stroke_moves_as_a_notch(cx: &mut TestAppContext) {
    let (shell, cx) = mounted(cx);
    let at = inside(&shell, cx);
    // GPUI triples a raw -10 axis value; Qt's angle is 12 units per raw unit, so -120 is one notch.
    cx.simulate_event(wheel(at, pixels(-30.), TouchPhase::Moved));
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.pane.scroll_y, 288.);
        assert!(
            !host.input.kinetic.running(),
            "a phaseless frame leaves no tail"
        );
    });
}

#[gpui::test]
fn g1_05_a_stroke_drags_at_raw_gain_and_a_lift_coasts_until_a_foreign_write(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = mounted(cx);
    let at = inside(&shell, cx);
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().pane.scroll_to(4000.)
    });
    cx.simulate_event(wheel(at, pixels(0.), TouchPhase::Started));
    for _ in 0..6 {
        cx.executor().advance_clock(Duration::from_millis(8));
        cx.simulate_event(wheel(at, pixels(-24.), TouchPhase::Moved));
    }
    let lifted = shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(
            host.pane.scroll_y,
            4000. + 6. * 20.,
            "a raw 8 px delta drags 20 px"
        );
        host.pane.scroll_y
    });
    cx.simulate_event(wheel(at, pixels(0.), TouchPhase::Ended));
    shell.read_with(cx, |shell, _| {
        assert!(shell.panes[0].as_ref().unwrap().input.kinetic.tail_active());
    });
    frames(&shell, cx, 3, 16);
    let coasted = shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(
            host.pane.scroll_y > lifted,
            "the tail keeps moving after the lift"
        );
        assert!(host.input.kinetic.tail_active());
        host.pane.scroll_y
    });
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().pane.scroll_to(100.)
    });
    frames(&shell, cx, 2, 16);
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(
            !host.input.moving(),
            "a write the motion did not make ends it"
        );
        assert_eq!(host.pane.scroll_y, 100.);
        assert!(coasted > lifted);
    });
}

#[gpui::test]
fn g1_05_a_stroke_past_the_end_rubber_bands_and_returns_on_outcubic(cx: &mut TestAppContext) {
    let (shell, cx) = mounted(cx);
    let at = inside(&shell, cx);
    let max = shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_mut().unwrap();
        let max = host.input.frame.unwrap().max_scroll();
        host.pane.scroll_to(max);
        max
    });
    cx.simulate_event(wheel(at, pixels(0.), TouchPhase::Started));
    cx.executor().advance_clock(Duration::from_millis(8));
    cx.simulate_event(wheel(at, pixels(-120.), TouchPhase::Moved));
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(
            host.pane.scroll_y, max,
            "the pane keeps the clamped position"
        );
        let drawn = host.input.drawn(host.pane.scroll_y);
        assert!(
            drawn > max && drawn < max + 100.,
            "past the end the travel resists"
        );
    });
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.simulate_event(wheel(at, pixels(0.), TouchPhase::Ended));
    shell.read_with(cx, |shell, _| {
        assert!(shell.panes[0]
            .as_ref()
            .unwrap()
            .input
            .kinetic
            .return_active());
    });
    frames(&shell, cx, 12, 16);
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(!host.input.moving());
        assert_eq!(host.input.over, 0.);
        assert_eq!(host.input.drawn(host.pane.scroll_y), max);
    });
}

#[gpui::test]
fn g1_05_a_lift_a_busy_pane_drops_still_closes_the_stroke(cx: &mut TestAppContext) {
    let (shell, cx) = mounted(cx);
    let at = inside(&shell, cx);
    cx.simulate_event(wheel(at, pixels(0.), TouchPhase::Started));
    shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_mut().unwrap();
        assert!(host.input.stroke);
        let now = host.pane.now();
        assert!(host.pane.begin("/next".into(), Default::default(), now));
        assert!(host.pane.busy());
    });
    cx.simulate_event(wheel(at, pixels(0.), TouchPhase::Ended));
    shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_mut().unwrap();
        assert!(
            !host.input.stroke,
            "membership reads the lift the busy pane refused"
        );
        let now = host.pane.now();
        host.pane.swap.end(now, "test");
        assert!(!host.pane.busy());
    });
    cx.simulate_event(wheel(at, pixels(-30.), TouchPhase::Moved));
    shell.read_with(cx, |shell, _| {
        assert_eq!(
            shell.panes[0].as_ref().unwrap().pane.scroll_y,
            288.,
            "a later frame alone is a notch again"
        );
    });
}

#[gpui::test]
fn g1_05_a_press_mid_glide_lands_the_view_where_drawn_and_takes_the_row_under_it(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = mounted(cx);
    let at = inside(&shell, cx);
    cx.simulate_event(wheel(
        at,
        ScrollDelta::Lines(point(0., -3.)),
        TouchPhase::Moved,
    ));
    frames(&shell, cx, 1, 90);
    let (target, drawn) = shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(host.input.glide.is_some());
        (host.pane.scroll_y, host.input.drawn(host.pane.scroll_y))
    });
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        let frame = host.input.frame.unwrap();
        assert!(host.input.glide.is_none(), "a press ends the glide");
        assert_eq!(
            host.pane.scroll_y, drawn,
            "the view stays where it was drawn"
        );
        let (_, y) = frame.local(at);
        let under = ((y + drawn) / frame.row).floor() as usize;
        assert_ne!(
            under,
            ((y + target) / frame.row).floor() as usize,
            "the probe tells the two positions apart"
        );
        assert_eq!(
            host.input.press.as_ref().unwrap().row,
            under,
            "the press takes the row drawn under the pointer"
        );
    });
}

/// Strokes past the end, lifts, lets the return run two frames and presses; the press holds the view past the end.
fn held_past_end(
    shell: &Entity<Shell>,
    cx: &mut gpui::VisualTestContext,
    at: gpui::Point<gpui::Pixels>,
) -> f32 {
    let max = shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_mut().unwrap();
        let max = host.input.frame.unwrap().max_scroll();
        host.pane.scroll_to(max);
        max
    });
    cx.simulate_event(wheel(at, pixels(0.), TouchPhase::Started));
    cx.executor().advance_clock(Duration::from_millis(8));
    cx.simulate_event(wheel(at, pixels(-120.), TouchPhase::Moved));
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.simulate_event(wheel(at, pixels(0.), TouchPhase::Ended));
    frames(shell, cx, 2, 16);
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    let held = shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(!host.input.moving(), "a press holds the return");
        host.input.drawn(host.pane.scroll_y)
    });
    assert!(held > max, "the press holds the view past the end");
    max
}

#[gpui::test]
fn g1_05_a_release_returns_an_overscroll_a_press_held(cx: &mut TestAppContext) {
    let (shell, cx) = mounted(cx);
    let at = inside(&shell, cx);
    let max = held_past_end(&shell, cx, at);
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    shell.read_with(cx, |shell, _| {
        assert!(
            shell.panes[0]
                .as_ref()
                .unwrap()
                .input
                .kinetic
                .return_active(),
            "the release starts the return"
        );
    });
    frames(&shell, cx, 12, 16);
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(!host.input.moving());
        assert_eq!(host.input.over, 0.);
        assert_eq!(host.input.drawn(host.pane.scroll_y), max);
    });
}

#[gpui::test]
fn g1_05_a_release_a_busy_pane_refuses_still_returns_the_overscroll(cx: &mut TestAppContext) {
    let (shell, cx) = mounted(cx);
    let at = inside(&shell, cx);
    held_past_end(&shell, cx, at);
    let before = shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_mut().unwrap();
        let row = host.input.press.as_ref().unwrap().row;
        assert!(
            host.selected.is_empty() && host.pane.cursor != row,
            "a click on the pressed row would mark and move"
        );
        let now = host.pane.now();
        assert!(host.pane.begin("/next".into(), Default::default(), now));
        assert!(host.pane.busy());
        (host.pane.cursor, host.selected.clone())
    });
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(
            (host.pane.cursor, host.selected.clone()),
            before,
            "the busy pane refused the click"
        );
        assert!(
            host.input.kinetic.return_active(),
            "the refused release still starts the return"
        );
    });
}

#[test]
fn g1_05_kinetic_policy_keeps_the_named_wheel_defaults() {
    assert_eq!(scroll::NOTCH_PX, f64::from(crate::input::NOTCH_PX));
    assert_eq!(
        scroll::MULTIPLIER,
        f64::from(crate::input::WHEEL_MULTIPLIER)
    );
    assert_eq!(scroll::TOUCH_GAIN, f64::from(crate::input::TOUCH_GAIN));
}

#[path = "g1_05_slow_tests.rs"]
mod slow_tests;
