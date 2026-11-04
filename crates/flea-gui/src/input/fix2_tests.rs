use super::*;
use gpui::{Entity, ScrollDelta, ScrollWheelEvent};
use std::time::Duration;

fn wheeled(cx: &mut TestAppContext) -> (Entity<Shell>, &mut gpui::VisualTestContext) {
    let (backend, _) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let shell = shell_with(backend, cx);
        window.focus(&shell.focus, cx);
        shell
    });
    cx.simulate_resize(size(px(900.), px(600.)));
    cx.run_until_parked();
    let frame = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().input.frame.unwrap()
    });
    let at = point(
        frame.bounds.left() + px(60.),
        frame.bounds.top() + px(frame.row * 2.5),
    );
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.simulate_event(ScrollWheelEvent {
        position: at,
        delta: ScrollDelta::Lines(gpui::point(0., -1.)),
        ..Default::default()
    });
    cx.run_until_parked();
    // The notch's OutCubic glide owns frames until it lands; the hold below starts from the notch.
    cx.executor().advance_clock(GLIDE);
    cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.run_until_parked();
    let moving = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().input.moving()
    });
    assert!(!moving);
    (shell, cx)
}

const GLIDE: Duration = Duration::from_millis(crate::input::scroll::RETURN_MS as u64);

#[gpui::test]
fn fix2_deadline_drives_the_whole_fade_without_refresh(cx: &mut TestAppContext) {
    let (shell, cx) = wheeled(cx);
    let rest = Duration::from_millis(999) - GLIDE;
    cx.executor().advance_clock(rest);
    cx.run_until_parked();
    assert_eq!(cx.update(|window, cx| window.simulate_next_frame(cx)), 0);
    cx.executor().advance_clock(Duration::from_millis(1));
    cx.run_until_parked();
    assert!(cx.update(|window, cx| window.simulate_next_frame(cx)) > 0);
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(100));
    assert!(cx.update(|window, cx| window.simulate_next_frame(cx)) > 0);
    cx.run_until_parked();
    shell.read_with(cx, |shell, cx| {
        let input = &shell.panes[0].as_ref().unwrap().input;
        let opacity = input.opacity(cx.background_executor().now(), false);
        assert!((opacity - 2. / 3.).abs() < 0.001);
    });
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();
    cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.run_until_parked();
    assert_eq!(cx.update(|window, cx| window.simulate_next_frame(cx)), 0);
    shell.read_with(cx, |shell, cx| {
        let input = &shell.panes[0].as_ref().unwrap().input;
        assert_eq!(input.opacity(cx.background_executor().now(), false), 0.);
        assert!(input.moved_at.is_none());
        assert!(input.timer.is_none());
    });
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();
    assert_eq!(cx.update(|window, cx| window.simulate_next_frame(cx)), 0);
}

#[gpui::test]
fn fix2_hover_stays_idle_across_hold_and_fade_deadlines(cx: &mut TestAppContext) {
    let (shell, cx) = wheeled(cx);
    let lane = shell.read_with(cx, |shell, _| {
        let frame = shell.panes[0].as_ref().unwrap().input.frame.unwrap();
        point(frame.bounds.right() - px(3.), frame.bounds.top() + px(50.))
    });
    cx.simulate_mouse_move(lane, None, Modifiers::none());
    cx.run_until_parked();
    for delay in [0, 1000 - GLIDE.as_millis() as u64, 300, 200] {
        cx.executor().advance_clock(Duration::from_millis(delay));
        cx.run_until_parked();
        shell.read_with(cx, |shell, cx| {
            let input = &shell.panes[0].as_ref().unwrap().input;
            let now = cx.background_executor().now();
            assert!(input.in_lane);
            assert_eq!(input.opacity(now, false), 1.);
            assert!(!input.animating(now, false));
        });
        assert_eq!(cx.update(|window, cx| window.simulate_next_frame(cx)), 0);
    }
}

#[gpui::test]
fn fix2_fade_includes_submillisecond_boundary_and_respects_reduced_motion(cx: &mut TestAppContext) {
    let (shell, cx) = wheeled(cx);
    shell.read_with(cx, |shell, _| {
        let input = &shell.panes[0].as_ref().unwrap().input;
        let start = input.moved_at.unwrap();
        assert!(!input.animating(start + Duration::from_micros(999_999), false));
        for elapsed in [1_000_000, 1_000_500] {
            let now = start + Duration::from_micros(elapsed);
            assert_eq!(input.opacity(now, false), 1.);
            assert!(input.animating(now, false));
            assert!(!input.animating(now, true));
        }
        assert!(!input.animating(start + Duration::from_millis(1300), false));
    });
}
