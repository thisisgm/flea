//! Mounted regression cases sharing the parent fixture.
use super::*;

#[gpui::test]
fn fix1_motion_without_a_hover_change_notifies_nothing(cx: &mut TestAppContext) {
    let (shell, cx) = mounted(cx);
    let frame = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().input.frame.unwrap()
    });
    let notified = Rc::new(std::cell::Cell::new(0usize));
    let seen = notified.clone();
    let _subscription = cx.update(|_, cx| cx.observe(&shell, move |_, _| seen.set(seen.get() + 1)));
    let row = |x: f32| {
        point(
            frame.bounds.left() + px(x),
            frame.bounds.top() + px(frame.row * 2.5),
        )
    };
    cx.simulate_mouse_move(row(40.), None, Modifiers::none());
    cx.run_until_parked();
    let first = notified.get();
    assert!(first > 0, "entering a row changes its hover");
    for x in [50., 60., 70., 80., 90.] {
        cx.simulate_mouse_move(row(x), None, Modifiers::none());
    }
    cx.run_until_parked();
    assert_eq!(
        notified.get(),
        first,
        "five moves inside one row notified {}",
        notified.get() - first
    );
    let chrome = point(frame.bounds.left() + px(40.), px(5.));
    cx.simulate_mouse_move(chrome, None, Modifiers::none());
    cx.run_until_parked();
    let left = notified.get();
    for x in [50., 60., 70., 80.] {
        cx.simulate_mouse_move(point(px(x), px(5.)), None, Modifiers::none());
    }
    cx.run_until_parked();
    assert_eq!(
        notified.get(),
        left,
        "motion over the chrome notified the shell"
    );
}

// r1-5: SelectionBand.qml:122-128 steps the edge scroll per presented frame by frame time, at any refresh rate.
#[gpui::test]
fn fix1_band_edge_scroll_steps_per_presented_frame_by_measured_time(cx: &mut TestAppContext) {
    let (shell, cx) = mounted(cx);
    let frame = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().input.frame.unwrap()
    });
    cx.update(|_, cx| {
        shell.update(cx, |shell, cx| {
            let host = shell.panes[0].as_mut().unwrap();
            host.input.band = Some(crate::input::Band {
                start: frame.bounds.origin,
                pointer: point(
                    frame.bounds.left() + px(60.),
                    frame.bounds.bottom() + px(40.),
                ),
                anchor: (60., 10.),
                end: (60., 10.),
                before: Vec::new(),
                cursor: 0,
                active: true,
                additive: false,
                shift: false,
                direction: 1.,
                last: None,
                cells: None,
                basis: None,
            });
            cx.notify();
        })
    });
    cx.run_until_parked();
    for _ in 0..3 {
        cx.executor().advance_clock(Duration::from_millis(8));
        cx.update(|window, cx| window.simulate_next_frame(cx));
        cx.run_until_parked();
    }
    let scroll = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().pane.scroll_y
    });
    let wanted = crate::input::EDGE_RATE * 0.024;
    assert!(
        (scroll - wanted).abs() < 0.5,
        "scrolled {scroll}px in three 8ms frames, wanted {wanted}"
    );
}

// r1-5: the bar's 300ms fade runs on presented frames and the window draws nothing once it ends.
#[gpui::test]
fn fix1_bar_fade_runs_on_frames_and_settles_idle(cx: &mut TestAppContext) {
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
    let draw = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        })
    };
    cx.executor().advance_clock(Duration::from_millis(1100));
    cx.run_until_parked();
    draw(cx);
    let fading = cx.update(|window, cx| window.simulate_next_frame(cx));
    assert!(fading > 0, "the fade asks for presented frames");
    cx.executor().advance_clock(Duration::from_millis(400));
    cx.run_until_parked();
    cx.update(|window, cx| window.simulate_next_frame(cx));
    draw(cx);
    let idle = cx.update(|window, cx| window.simulate_next_frame(cx));
    assert_eq!(idle, 0, "a settled bar requests no frame");
}

// r1-8: the list frame starts where the composition puts the pane's rows.
#[gpui::test]
fn fix1_list_frame_follows_the_composition_rows(cx: &mut TestAppContext) {
    let (shell, cx) = mounted(cx);
    let (frame, rows) = cx.update(|window, cx| {
        shell.update(cx, |shell, _| {
            let frame = shell.panes[0].as_ref().unwrap().input.frame.unwrap();
            let rows = shell
                .geometry(window)
                .rows(0, false, shell.slots.strip_height);
            (frame, rows)
        })
    });
    assert_eq!(f32::from(frame.bounds.origin.x), rows.x);
    assert_eq!(f32::from(frame.bounds.origin.y), rows.y);
}
