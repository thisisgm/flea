use super::*;
use gpui::{Entity, ScrollDelta, ScrollWheelEvent};
use std::time::Duration;

fn mounted(cx: &mut TestAppContext) -> (Entity<Shell>, &mut gpui::VisualTestContext) {
    let (backend, _) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let shell = shell_with(backend, cx);
        window.focus(&shell.focus, cx);
        shell
    });
    cx.simulate_resize(size(px(900.), px(600.)));
    cx.run_until_parked();
    (shell, cx)
}

#[gpui::test]
fn closing_bar_fade_requests_a_frame_at_the_hold_deadline(cx: &mut TestAppContext) {
    let (shell, cx) = mounted(cx);
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
    // The notch's OutCubic glide owns frames until it lands; the hold is measured from the notch.
    let glide = Duration::from_millis(crate::input::scroll::RETURN_MS as u64);
    cx.executor().advance_clock(glide);
    cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.run_until_parked();
    let before = cx.update(|window, cx| window.simulate_next_frame(cx));
    assert_eq!(before, 0, "the hold correctly needs no animation frames");
    let rest = Duration::from_millis(1000) - glide;
    cx.executor().advance_clock(rest);
    cx.run_until_parked();
    let (elapsed, opacity, animating) = cx.update(|_, cx| {
        shell.read_with(cx, |shell, cx| {
            let input = &shell.panes[0].as_ref().unwrap().input;
            let now = cx.background_executor().now();
            (
                now.saturating_duration_since(input.moved_at.unwrap())
                    .as_millis(),
                input.opacity(now, false),
                input.animating(now, false),
            )
        })
    });
    let pending = cx.update(|window, cx| window.simulate_next_frame(cx));
    eprintln!("CLOSING hold deadline elapsed={elapsed} opacity={opacity} animating={animating} requested_callbacks={pending}");
    assert!(
        pending > 0,
        "the hold wake must schedule the 300ms fade without a forced refresh"
    );
}

#[gpui::test]
fn closing_refresh_cancels_an_edge_band_before_advancing_it(cx: &mut TestAppContext) {
    let (backend, _receiver) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let mut shell = shell_with(backend, cx);
        let host = shell.panes[0].as_mut().unwrap();
        host.pane.held = Rc::new(crate::list::pane::Held {
            listing: 2,
            total: 40,
            start: 0,
            rows: vec![Default::default(); 40],
        });
        host.changed(&shell.locale);
        window.focus(&shell.focus, cx);
        shell
    });
    cx.simulate_resize(size(px(900.), px(600.)));
    cx.run_until_parked();
    cx.simulate_keystrokes("end");
    cx.run_until_parked();
    let frame = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().input.frame.unwrap()
    });
    let at = point(
        frame.bounds.left() + px(100.),
        frame.bounds.bottom() - px(3.),
    );
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.simulate_event(ScrollWheelEvent {
        position: at,
        delta: ScrollDelta::Lines(gpui::point(0., -1.)),
        ..Default::default()
    });
    cx.run_until_parked();
    // Finish the notch glide before the real gesture targets the empty footer below the last row.
    cx.executor().advance_clock(Duration::from_millis(
        crate::input::scroll::RETURN_MS as u64,
    ));
    cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| {
        let input = &shell.panes[0].as_ref().unwrap().input;
        assert!(
            !input.moving(),
            "the empty-footer gesture starts after the notch settles"
        );
        assert_eq!(input.hit(at), None, "the gesture starts below the last row");
    });
    shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.selected = vec![2, 4];
        host.input.selection.sync(&host.selected);
    });
    let outside = point(frame.bounds.left() + px(50.), frame.bounds.top() - px(40.));
    cx.simulate_mouse_move(at, None, Modifiers::none());
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        eprintln!(
            "CLOSING gesture start frame={:?} hit={:?} busy={} stopped={}",
            host.input.frame,
            host.input.hit(at),
            host.pane.busy(),
            host.stopped
        );
    });
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(outside, Some(MouseButton::Left), Modifiers::none());
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(host
            .input
            .band
            .as_ref()
            .is_some_and(|b| b.active && b.direction < 0.));
        assert_ne!(
            host.selected,
            vec![2, 4],
            "the real pointer gesture changed the marks"
        );
    });
    let (scroll_before_refresh, cursor_before_gesture) = shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        (host.pane.scroll_y, host.input.band.as_ref().unwrap().cursor)
    });
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert!(shell
                .step(0, crate::window::navigate::Step::Refresh(None), window, cx)
                .unwrap());
            assert!(shell.panes[0].as_ref().unwrap().pane.busy());
        })
    });
    cx.run_until_parked();
    for _ in 0..3 {
        cx.executor().advance_clock(Duration::from_millis(8));
        cx.update(|window, cx| window.simulate_next_frame(cx));
        cx.run_until_parked();
    }
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        eprintln!(
            "CLOSING refresh busy={} band_active={} scroll={} selected={:?}",
            host.pane.busy(),
            host.input.band.as_ref().is_some_and(|b| b.active),
            host.pane.scroll_y,
            host.selected
        );
        assert!(!host.stopped, "the backend channel remained open");
        assert!(
            host.input.band.is_none(),
            "a pending listing must cancel the old edge gesture"
        );
        assert_eq!(
            host.selected,
            [2, 4],
            "cancel restores the pre-gesture marks"
        );
        assert_eq!(
            host.pane.scroll_y, scroll_before_refresh,
            "pending frames cannot advance the old gesture"
        );
        assert_eq!(
            host.pane.cursor, cursor_before_gesture,
            "cancel restores the pre-gesture cursor"
        );
        assert_eq!(host.input.selection.snapshot.as_deref(), Some(&[2, 4][..]));
        for row in host.list.borrow().visible() {
            assert_eq!(
                row.state.selected,
                [2, 4].contains(&row.index),
                "row snapshots use restored marks"
            );
        }
    });
}
