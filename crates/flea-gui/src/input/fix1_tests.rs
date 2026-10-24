//! FIX1 for the round 1 review: each test fails on the round 1 code for the reason its comment names.
use super::*;
use gpui::{Entity, Modifiers, MouseButton, ScrollDelta, ScrollWheelEvent};
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

// List.qml:213-231 clamps the cursor on every contentY change outside a band, so a track press pulls it into the view.
#[gpui::test]
fn review_track_press_pulls_the_cursor_into_the_view(cx: &mut TestAppContext) {
    let (shell, cx) = mounted(cx);
    let frame = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().input.frame.unwrap()
    });
    let lane = point(
        frame.bounds.right() - px(3.),
        frame.bounds.top() + frame.bounds.size.height / 8.,
    );
    cx.simulate_mouse_move(lane, None, Modifiers::none());
    cx.simulate_mouse_down(lane, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(lane, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    let (scroll, cursor) = shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        (host.pane.scroll_y, host.pane.cursor)
    });
    let first = (scroll / frame.row).floor() as usize;
    eprintln!("REVIEW track press scroll={scroll} cursor={cursor} first_visible={first}");
    assert!(scroll > frame.viewport(), "the track press scrolled");
    cx.update(|window, cx| shell.update(cx, |shell, cx| super::key(shell, "down", window, cx)));
    cx.run_until_parked();
    let (after, moved) = shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        (host.pane.scroll_y, host.pane.cursor)
    });
    eprintln!("REVIEW after down scroll={after} cursor={moved}");
    assert!(
        cursor >= first,
        "the track press left the cursor on row {cursor}, above the first visible row {first}"
    );
    // ScrollOff.js keeps three rows above a keyboard cursor, so Down may pull the view up by them and no more.
    assert!(
        after >= scroll - 4. * frame.row,
        "Down after a track press jumped the view from {scroll} to {after}"
    );
}

// List.qml:220-231 with Pane.qml:281: first is floor(contentY / row), so a cut top row keeps the cursor.
#[gpui::test]
fn review_wheel_clamp_keeps_the_cut_top_row(cx: &mut TestAppContext) {
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
        delta: ScrollDelta::Lines(gpui::point(0., -3.)),
        ..Default::default()
    });
    cx.run_until_parked();
    let (scroll, cursor) = shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        (host.pane.scroll_y, host.pane.cursor)
    });
    let qml_first = (scroll / frame.row).floor() as usize;
    eprintln!(
        "REVIEW wheel scroll={scroll} row={} cursor={cursor} qml_first={qml_first}",
        frame.row
    );
    assert_eq!(scroll, 288.);
    assert_eq!(cursor, qml_first, "List.qml clamps to the cut top row");
}

// Focus.js:168 pages floor(visibleRows / 2), and Pane.qml:281 has visibleRows = ceil(list.height / row).
#[gpui::test]
fn review_page_down_steps_half_of_the_ceiled_visible_rows(cx: &mut TestAppContext) {
    let (shell, cx) = mounted(cx);
    let mut checked = None;
    for height in 560..660 {
        cx.simulate_resize(size(px(900.), px(height as f32)));
        cx.run_until_parked();
        let frame = shell.read_with(cx, |shell, _| {
            shell.panes[0].as_ref().unwrap().input.frame.unwrap()
        });
        let rows = frame.viewport() / frame.row;
        let qml = ((rows.ceil() as usize) / 2).max(1);
        if qml != ((rows / 2.).floor() as usize).max(1) {
            checked = Some((height, frame.viewport(), qml));
            break;
        }
    }
    let (height, viewport, qml) = checked.expect("a height where the two rules differ");
    cx.update(|window, cx| shell.update(cx, |shell, cx| super::key(shell, "ctrl-d", window, cx)));
    cx.run_until_parked();
    let cursor = shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().pane.cursor);
    eprintln!("REVIEW page height={height} viewport={viewport} cursor={cursor} qml={qml}");
    assert_eq!(cursor, qml, "Ctrl+D pages floor(ceil(viewport / row) / 2)");
}

// Filter.js:254-258: the pull anchors a Shift range only while no move, mark or new listing came since.
#[gpui::test]
fn review_wheel_pull_does_not_anchor_a_range_in_the_next_listing(cx: &mut TestAppContext) {
    let (shell, cx) = mounted(cx);
    for _ in 0..5 {
        cx.update(|window, cx| shell.update(cx, |shell, cx| super::key(shell, "down", window, cx)));
    }
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
        delta: ScrollDelta::Lines(gpui::point(0., -3.)),
        ..Default::default()
    });
    cx.run_until_parked();
    let pulled = shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().pane.cursor);
    assert!(pulled > 5, "the wheel pulled the cursor off row 5");
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.panes[0].as_mut().unwrap().pane.begin(
                "/next".into(),
                crate::pane::swap::Options::default(),
                0,
            );
            shell.receive(
                0,
                BackendEvent::Reply(Reply::Listed {
                    path: "/next".into(),
                    total: 40,
                }),
                window,
                cx,
            );
            shell.receive(
                0,
                BackendEvent::Reply(Reply::Rows {
                    start: 0,
                    rows: vec![flea::protocol::RowFacts::default(); 40],
                    listing: 3,
                }),
                window,
                cx,
            );
        })
    });
    cx.run_until_parked();
    let (listing, cursor) = shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        (host.pane.held.listing, host.pane.cursor)
    });
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| super::key(shell, "shift-down", window, cx))
    });
    let selected = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().selected.clone()
    });
    eprintln!(
        "REVIEW new listing={listing} cursor={cursor} pulled_before={pulled} selected={selected:?}"
    );
    assert_eq!(
        selected,
        [cursor, cursor + 1],
        "a fresh listing anchors at its own cursor"
    );
}

// r1-1: a held knob drag pulls the cursor into the view like every other non-band scroll.
#[gpui::test]
fn fix1_knob_drag_pulls_the_cursor_into_the_view(cx: &mut TestAppContext) {
    let (shell, cx) = mounted(cx);
    let frame = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().input.frame.unwrap()
    });
    let grab = point(frame.bounds.right() - px(3.), frame.bounds.top() + px(10.));
    let drag = point(grab.x, grab.y + px(200.));
    cx.simulate_mouse_move(grab, None, Modifiers::none());
    cx.simulate_mouse_down(grab, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(drag, Some(MouseButton::Left), Modifiers::none());
    cx.run_until_parked();
    let (scroll, cursor) = shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        (host.pane.scroll_y, host.pane.cursor)
    });
    assert!(scroll > frame.viewport(), "the knob drag scrolled");
    assert!(
        cursor >= (scroll / frame.row).floor() as usize,
        "cursor {cursor} left above the view"
    );
}

// r1-6: Ipc.qml:90 Style.selectedFill is the wash Row.qml:127 paints under a marked row in a pane without focus.
#[gpui::test]
fn fix1_selected_fill_is_the_wash_the_list_paints_for_an_unfocused_marked_row(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = mounted(cx);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            super::key(shell, "v", window, cx);
            super::key(shell, "down", window, cx);
        });
        window.blur(cx);
    });
    let (quads, scale) = cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        (window.painted_quads(), window.scale_factor())
    });
    let (fill, frame, selected, cursor) = shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        let drawn = host.input.drawn.unwrap();
        (
            drawn.fill,
            drawn.frame,
            host.selected.clone(),
            host.pane.cursor,
        )
    });
    assert_eq!(
        (selected, cursor),
        (vec![0], 1),
        "row 0 is marked, the cursor is on row 1"
    );
    let wanted = format!("{fill:?}");
    let on_marked_row = quads.iter().any(|quad| {
        quad.bounds.origin.y == frame.bounds.top().scale(scale)
            && format!("{:?}", quad.background).contains(&wanted)
    });
    assert!(
        on_marked_row,
        "the reader names a wash the marked row never painted: {fill:?}"
    );
}

// r1-7: motion that changes no hover, lane or gesture draws nothing, so it must not notify the shell.
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
                direction: 1.,
                last: None,
                cells: None,
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
