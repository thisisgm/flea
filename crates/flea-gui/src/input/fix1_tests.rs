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

// S2-5 (FX-KEYS) replaced List.qml's floor(contentY / row): a cut top row passes the cursor to the first whole row.
#[gpui::test]
fn review_wheel_clamp_moves_the_cursor_below_the_cut_top_row(cx: &mut TestAppContext) {
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
    let cut = (scroll / frame.row).floor() as usize;
    eprintln!(
        "REVIEW wheel scroll={scroll} row={} cursor={cursor} cut_top={cut}",
        frame.row
    );
    assert_eq!(scroll, 288.);
    assert!(
        scroll > cut as f32 * frame.row,
        "the wheel must leave the top row cut for this test to prove S2-5"
    );
    assert_eq!(cursor, cut + 1, "S2-5 clamps to the first wholly drawn row");
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

// r1-6, EY-417: Row.qml:127 greys marks only in the pane that is not current, so a blurred window keeps the plate.
#[gpui::test]
fn fix1_a_blurred_window_keeps_the_marked_plate_under_a_marked_row(cx: &mut TestAppContext) {
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
            host.list.borrow().drawn.palette.unwrap().marked,
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
        "a blurred window greyed the marked row: {fill:?} never painted"
    );
}

// EY-210: selectedFill names the grey selected fill token the cursor and rail paint, the same in every focus state.
#[gpui::test]
fn ey210_selected_fill_names_the_grey_token_in_each_focus_state(cx: &mut TestAppContext) {
    use crate::ipc::Adapter;
    let (shell, cx) = mounted(cx);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            super::key(shell, "v", window, cx);
            super::key(shell, "down", window, cx);
        });
    });
    for focused in [true, false] {
        cx.update(|window, cx| {
            if !focused {
                window.blur(cx);
            }
            window.refresh();
            window.draw(cx).clear(cx);
        });
        let (quads, scale) = cx.update(|window, _| (window.painted_quads(), window.scale_factor()));
        let (fill, palette, top) = shell.read_with(cx, |shell, _| {
            let host = shell.panes[0].as_ref().unwrap();
            let palette = host.list.borrow().drawn.palette.unwrap();
            let reply = crate::ipc::focus::FocusAdapter {
                view: Some("list"),
                input: &host.input,
                chrome_glyph: None,
            }
            .call("selectedFill", &[])
            .unwrap();
            (
                reply.stdout.trim().to_owned(),
                palette,
                host.input.frame.unwrap().bounds.top(),
            )
        });
        let grey = palette.foreground.opacity(crate::list::GREY_CURSOR);
        assert_eq!(
            fill,
            crate::ipc::rows::colour(grey),
            "focused {focused}: selectedFill names the grey selected fill token"
        );
        assert_ne!(
            fill,
            crate::ipc::rows::colour(palette.marked),
            "focused {focused}: selectedFill is not the focused marked plate"
        );
        // EY-417: Row.qml:127 keeps the marked plate in the current pane whatever the window's focus.
        let painted = format!("{:?}", palette.marked);
        assert!(
            quads.iter().any(|quad| {
                quad.bounds.origin.y == top.scale(scale)
                    && format!("{:?}", quad.background).contains(&painted)
            }),
            "focused {focused}: the marked row never painted {painted}"
        );
        if !focused {
            let painted = format!("{grey:?}");
            assert!(
                quads
                    .iter()
                    .any(|quad| format!("{:?}", quad.background).contains(&painted)),
                "unfocused: the cursor row never painted the grey token {painted}"
            );
        }
    }
}

// r1-7: motion that changes no hover, lane or gesture draws nothing, so it must not notify the shell.
#[path = "fix1_motion_tests.rs"]
mod extracted;
