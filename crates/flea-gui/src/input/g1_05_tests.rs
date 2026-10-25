use super::*;
use crate::input::{scroll, slow, Band};
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
    cx.simulate_event(wheel(at, ScrollDelta::Lines(point(0., -3.)), TouchPhase::Moved));
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.pane.scroll_y, 288., "three lines are one notch of 3 x 24 x 4");
        assert!(host.input.glide.is_some());
        assert!(host.input.drawn(host.pane.scroll_y) < 288., "the rows draw from where they stood");
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
        assert!(!host.input.kinetic.running(), "a phaseless frame leaves no tail");
    });
}

#[gpui::test]
fn g1_05_a_stroke_drags_at_raw_gain_and_a_lift_coasts_until_a_foreign_write(cx: &mut TestAppContext) {
    let (shell, cx) = mounted(cx);
    let at = inside(&shell, cx);
    shell.update(cx, |shell, _| shell.panes[0].as_mut().unwrap().pane.scroll_to(4000.));
    cx.simulate_event(wheel(at, pixels(0.), TouchPhase::Started));
    for _ in 0..6 {
        cx.executor().advance_clock(Duration::from_millis(8));
        cx.simulate_event(wheel(at, pixels(-24.), TouchPhase::Moved));
    }
    let lifted = shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.pane.scroll_y, 4000. + 6. * 20., "a raw 8 px delta drags 20 px");
        host.pane.scroll_y
    });
    cx.simulate_event(wheel(at, pixels(0.), TouchPhase::Ended));
    shell.read_with(cx, |shell, _| {
        assert!(shell.panes[0].as_ref().unwrap().input.kinetic.tail_active());
    });
    frames(&shell, cx, 3, 16);
    let coasted = shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(host.pane.scroll_y > lifted, "the tail keeps moving after the lift");
        assert!(host.input.kinetic.tail_active());
        host.pane.scroll_y
    });
    shell.update(cx, |shell, _| shell.panes[0].as_mut().unwrap().pane.scroll_to(100.));
    frames(&shell, cx, 2, 16);
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(!host.input.moving(), "a write the motion did not make ends it");
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
        assert_eq!(host.pane.scroll_y, max, "the pane keeps the clamped position");
        let drawn = host.input.drawn(host.pane.scroll_y);
        assert!(drawn > max && drawn < max + 100., "past the end the travel resists");
    });
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.simulate_event(wheel(at, pixels(0.), TouchPhase::Ended));
    shell.read_with(cx, |shell, _| {
        assert!(shell.panes[0].as_ref().unwrap().input.kinetic.return_active());
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
        host.input.band = Some(Band {
            start: at,
            anchor: (0., 0.),
            end: (0., 0.),
            before: Vec::new(),
            cursor: 0,
            active: false,
            additive: false,
            direction: 0.,
            last: None,
            cells: None,
            pointer: at,
        });
    });
    cx.simulate_event(wheel(at, pixels(0.), TouchPhase::Ended));
    shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_mut().unwrap();
        assert!(!host.input.stroke, "membership reads the refused lift");
        host.input.band = None;
    });
    cx.simulate_event(wheel(at, pixels(-30.), TouchPhase::Moved));
    shell.read_with(cx, |shell, _| {
        assert_eq!(shell.panes[0].as_ref().unwrap().pane.scroll_y, 288., "a later frame alone is a notch again");
    });
}

fn named(shell: &Entity<Shell>, cx: &mut gpui::VisualTestContext) {
    shell.update(cx, |shell, cx| {
        let host = shell.panes[0].as_mut().unwrap();
        let mut rows = host.pane.held.rows.clone();
        for (at, row) in rows.iter_mut().enumerate() {
            row.name = format!("file-{at:03}.txt");
        }
        host.pane.held = Rc::new(crate::list::pane::Held {
            listing: host.pane.held.listing,
            total: host.pane.held.total,
            start: 0,
            rows,
        });
        cx.notify();
    });
    cx.run_until_parked();
}

fn on_name(shell: &Entity<Shell>, cx: &mut gpui::VisualTestContext) -> gpui::Point<gpui::Pixels> {
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        let frame = host.input.frame.unwrap();
        let list = host.list.borrow();
        let x = list.drawn.bounds.origin.x + px(list.drawn.layout.name.x + 4.);
        assert!(list.on_name(2, x), "the probe point sits on the drawn name");
        assert!(!list.on_name(2, list.drawn.bounds.origin.x + px(list.drawn.layout.name.x + list.drawn.layout.name.width - 1.)));
        point(x, frame.bounds.top() + px(frame.row * 2.5))
    })
}

fn tap(cx: &mut gpui::VisualTestContext, at: gpui::Point<gpui::Pixels>) {
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
}

#[gpui::test]
fn g1_05_a_slow_second_tap_on_the_sole_marked_name_arms_the_rename(cx: &mut TestAppContext) {
    let (shell, cx) = mounted(cx);
    named(&shell, cx);
    let at = on_name(&shell, cx);
    cx.simulate_mouse_move(at, None, Modifiers::none());
    tap(cx, at);
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.selected, [2]);
        assert!(host.input.slow_task.is_none(), "the first tap only selects");
    });
    cx.executor().advance_clock(Duration::from_millis(500));
    tap(cx, at);
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(host.input.slow_task.is_some(), "a slow second tap arms the timer");
        assert_eq!(host.input.slow.index, 2);
    });
    cx.executor().advance_clock(Duration::from_millis(slow::DOUBLE_CLICK_MS + 10));
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(host.input.slow_task.is_none());
        assert_eq!(host.input.slow.index, slow::CLEARED, "the timer fired and cleared its record");
    });
}

#[gpui::test]
fn g1_05_a_quick_second_tap_or_a_tap_off_the_name_never_arms(cx: &mut TestAppContext) {
    let (shell, cx) = mounted(cx);
    named(&shell, cx);
    let at = on_name(&shell, cx);
    cx.simulate_mouse_move(at, None, Modifiers::none());
    tap(cx, at);
    cx.executor().advance_clock(Duration::from_millis(200));
    tap(cx, at);
    shell.read_with(cx, |shell, _| {
        assert!(shell.panes[0].as_ref().unwrap().input.slow_task.is_none(), "inside the interval is the double click");
    });
    let off = shell.read_with(cx, |shell, _| {
        let list = shell.panes[0].as_ref().unwrap().list.borrow();
        point(list.drawn.bounds.origin.x + px(list.drawn.layout.name.x + list.drawn.layout.name.width - 1.), at.y)
    });
    cx.executor().advance_clock(Duration::from_millis(500));
    tap(cx, off);
    cx.executor().advance_clock(Duration::from_millis(500));
    tap(cx, off);
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(host.input.slow_task.is_none(), "the empty rest of the name slot is not the name");
        assert_eq!(host.input.slow.index, slow::CLEARED);
    });
}
