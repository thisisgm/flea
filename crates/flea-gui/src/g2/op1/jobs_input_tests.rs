//! Real Root dispatch at measured card whitespace, with folded and outside-card controls.
use super::update;
use crate::{
    backend::{Backend, Event},
    columns::geometry::Slot,
    window::{root::Root, startup::View},
};
use gpui::{
    point, px, AppContext, Bounds, Modifiers, MouseButton, Pixels, Point, ScrollDelta,
    ScrollWheelEvent, TestAppContext, TestDispatcher, VisualTestContext, WindowHandle,
};

type Handle = WindowHandle<Root>;

fn frame(cx: &mut TestAppContext, handle: Handle) {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
}

fn bounds(cx: &mut TestAppContext, handle: Handle, name: &'static str) -> Bounds<Pixels> {
    VisualTestContext::from_window(handle.into(), cx)
        .debug_bounds(name)
        .unwrap()
}

fn open(view: View) -> (TestAppContext, Handle, impl FnMut() -> Option<String>) {
    let mut cx = TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        gpui_platform::current_platform(true).text_system(),
    );
    let (backend, mut received) = Backend::test_lines();
    let handle = super::super::atomic_tests::open_with(&mut cx, backend);
    VisualTestContext::from_window(handle.into(), &cx)
        .simulate_resize(gpui::size(px(1100.), px(700.)));
    update(&mut cx, handle, |shell, window, cx| {
        shell.launch.settings.columns = 3;
        shell.set_view(view, window, cx);
        if view == View::Columns {
            let host = shell.panes[0].as_ref().unwrap();
            let columns = host.columns.as_ref().unwrap();
            let folders = host.nav.folders.borrow();
            // Columns entry queued these watched peeks before the fixture supplies their replies.
            for path in &columns.area.drawn {
                let expected = crate::columns::peeks::Ask {
                    path,
                    first: host.pane.first_count(),
                    hidden: folders.hidden,
                    hidden_last: folders.hidden_last,
                    order: folders.peek_order(path),
                    keep: &columns.area.drawn,
                }
                .line();
                assert_eq!(
                    received().as_deref(),
                    Some(expected.as_str()),
                    "Columns setup must consume only its exact queued peek request"
                );
            }
            let replies: Vec<_> = columns.area.drawn.iter().map(|path| {
                format!(r#"{{"t":"peeked","path":"{}","first":{},"n":1,"hidden":{},"hiddenLast":{},"failed":false,"mode":0,"rows":[{{"n":"x","d":true}}]}}"#,
                    flea::json::escape(path), host.pane.first_count(), folders.hidden, folders.hidden_last)
            }).collect();
            drop(folders);
            for reply in replies {
                shell.receive(
                    0,
                    Event::Peeked(flea::jsondoc::parse(&reply).unwrap()),
                    window,
                    cx,
                );
            }
        }
    });
    frame(&mut cx, handle);
    let packet = received();
    assert!(
        packet.is_none(),
        "unexpected {view:?} setup packet: {packet:?}"
    );
    // A menu open also asks for the archive formats after its snapshot; the snapshot alone marks an open.
    let received = move || loop {
        let line = received()?;
        if !line.contains("\"c\":\"formats\"") {
            return Some(line);
        }
    };
    (cx, handle, received)
}

fn start(cx: &mut TestAppContext, handle: Handle) {
    super::super::atomic_tests::line(
        cx,
        handle,
        r#"{"t":"jobs","jobs":[{"id":13,"kind":"copy","head":"Copying a.bin","state":"running"},{"id":14,"kind":"copy","head":"Copying b.bin","state":"waiting"}]}"#,
    );
    super::super::atomic_tests::advance(cx, handle, 150);
    frame(cx, handle);
    assert!(VisualTestContext::from_window(handle.into(), cx)
        .debug_bounds("op1-scrim")
        .is_none());
}

fn fold(cx: &mut TestAppContext, handle: Handle) {
    update(cx, handle, |shell, _, cx| {
        shell
            .operation_view(0)
            .unwrap()
            .update(cx, |overlay, cx| overlay.fold_jobs(cx));
    });
    frame(cx, handle);
}

fn wheel(cx: &mut TestAppContext, handle: Handle, at: Point<Pixels>) {
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_mouse_move(at, None, Modifiers::none());
    visual.simulate_event(ScrollWheelEvent {
        position: at,
        delta: ScrollDelta::Lines(point(0., -3.)),
        modifiers: Modifiers::none(),
        touch_phase: gpui::TouchPhase::Moved,
    });
}

fn press(cx: &mut TestAppContext, handle: Handle, at: Point<Pixels>, button: MouseButton) {
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_mouse_move(at, None, Modifiers::none());
    visual.simulate_mouse_down(at, button, Modifiers::none());
    visual.simulate_mouse_up(at, button, Modifiers::none());
}

fn scroll(cx: &mut TestAppContext, handle: Handle) -> f32 {
    update(cx, handle, |shell, _, _| {
        shell.panes[0].as_ref().unwrap().pane.scroll_y
    })
}

fn reset(cx: &mut TestAppContext, handle: Handle) {
    update(cx, handle, |shell, window, cx| {
        shell.retire_menu(window, cx);
        let host = shell.panes[0].as_mut().unwrap();
        host.pane.cursor = 0;
        host.selected = vec![0];
        host.pane.scroll_to(0.);
        cx.notify();
    });
    frame(cx, handle);
}

fn column_row(cx: &mut TestAppContext, handle: Handle, at: Point<Pixels>) -> usize {
    update(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(!host.pane.busy());
        let paint = host
            .columns
            .as_ref()
            .unwrap()
            .painters
            .get(Slot::Active)
            .unwrap()
            .borrow();
        let drawn = paint.drawn.as_ref().unwrap();
        assert!(
            drawn.bounds.contains(&at),
            "card point must cover the real active track"
        );
        assert!(drawn.view().enabled && !host.nav.gate.owned());
        let row = ((f32::from(at.y - drawn.bounds.top()) + drawn.scroll)
            / crate::list::pane::ROW_HEIGHT)
            .floor() as usize;
        assert!(host.pane.held.row(row).is_some() && row != 0);
        row
    })
}

#[test]
fn jobs_padding_never_wheels_the_covered_active_column() {
    let (mut cx, handle, _received) = open(View::Columns);
    start(&mut cx, handle);
    let jobs = bounds(&mut cx, handle, "op1-jobs");
    let at = point(jobs.left() + px(5.), jobs.top() + px(5.));
    column_row(&mut cx, handle, at);
    fold(&mut cx, handle);
    assert!(VisualTestContext::from_window(handle.into(), &cx)
        .debug_bounds("op1-jobs")
        .is_none());
    wheel(&mut cx, handle, at);
    assert!(
        scroll(&mut cx, handle) > 0.,
        "folded control must scroll at the same point"
    );
    reset(&mut cx, handle);
    fold(&mut cx, handle);
    bounds(&mut cx, handle, "op1-jobs");
    wheel(&mut cx, handle, at);
    assert_eq!(
        scroll(&mut cx, handle),
        0.,
        "card padding leaked wheel to Columns"
    );
    let outside = point(at.x, jobs.top() - px(20.));
    column_row(&mut cx, handle, outside);
    wheel(&mut cx, handle, outside);
    assert!(
        scroll(&mut cx, handle) > 0.,
        "outside-card wheel must still reach Columns"
    );
    reset(&mut cx, handle);
    super::super::atomic_tests::line(&mut cx, handle, r#"{"t":"jobs","jobs":[]}"#);
    super::super::atomic_tests::advance(&mut cx, handle, 100);
    frame(&mut cx, handle);
    assert!(VisualTestContext::from_window(handle.into(), &cx)
        .debug_bounds("op1-jobs")
        .is_none());
    wheel(&mut cx, handle, at);
    assert!(
        scroll(&mut cx, handle) > 0.,
        "absent-card control must scroll"
    );
}

#[test]
fn jobs_whitespace_blocks_columns_left_and_right_press_but_keeps_cancel() {
    for button in [MouseButton::Left, MouseButton::Right] {
        let (mut cx, handle, mut received) = open(View::Columns);
        start(&mut cx, handle);
        reset(&mut cx, handle);
        let jobs = bounds(&mut cx, handle, "op1-jobs");
        let rule = bounds(&mut cx, handle, "op1-jobs-rule-1");
        let job = bounds(&mut cx, handle, "op1-job-13");
        let points = [
            point(jobs.left() + px(5.), jobs.top() + px(5.)),
            point(jobs.left() + px(5.), rule.center().y),
            point(jobs.left() + px(5.), job.bottom() - px(10.)),
        ];
        for at in points {
            let row = column_row(&mut cx, handle, at);
            press(&mut cx, handle, at, button);
            update(&mut cx, handle, |shell, _, _| {
                let host = shell.panes[0].as_ref().unwrap();
                assert_eq!(
                    (host.pane.cursor, host.selected.as_slice()),
                    (0, &[0][..]),
                    "covered press changed Columns: button={button:?}, point={at:?}"
                );
            });
            let packet = received();
            assert!(
                packet.is_none(),
                "covered right press must not request a menu snapshot: button={button:?}, point={at:?}, packet={packet:?}"
            );
            fold(&mut cx, handle);
            press(&mut cx, handle, at, button);
            update(&mut cx, handle, |shell, _, _| {
                assert_eq!(
                    shell.panes[0].as_ref().unwrap().pane.cursor,
                    row,
                    "folded control must change the real covered cursor"
                );
            });
            if button == MouseButton::Right {
                let packet = received();
                assert!(
                    packet
                        .as_deref()
                        .is_some_and(|line| line.contains("\"op\":\"snapshot\"")),
                    "folded control: button={button:?}, point={at:?}, packet={packet:?}"
                );
            }
            reset(&mut cx, handle);
            while received().is_some() {}
            fold(&mut cx, handle);
        }
        let outside = point(jobs.left() + px(5.), jobs.top() - px(20.));
        let row = column_row(&mut cx, handle, outside);
        press(&mut cx, handle, outside, button);
        update(&mut cx, handle, |shell, _, _| {
            assert_eq!(
                shell.panes[0].as_ref().unwrap().pane.cursor,
                row,
                "outside-card press must still reach Columns"
            );
        });
        if button == MouseButton::Right {
            let packet = received();
            assert!(
                packet
                    .as_deref()
                    .is_some_and(|line| line.contains("\"op\":\"snapshot\"")),
                "outside control: button={button:?}, point={outside:?}, packet={packet:?}"
            );
        }
        reset(&mut cx, handle);
        while received().is_some() {}
        let cancel = point(job.right() - px(5.), job.bottom() - px(10.));
        press(&mut cx, handle, cancel, MouseButton::Left);
        assert_eq!(
            received().as_deref(),
            Some(r#"{"c":"jobs","op":"cancel","id":13}"#),
            "real Cancel: button=Left, point={cancel:?}"
        );
        assert!(received().is_none());
    }
}

#[test]
fn jobs_padding_blocks_grid_press_at_a_measured_tile_overlap() {
    let (mut cx, handle, _received) = open(View::Grid);
    start(&mut cx, handle);
    reset(&mut cx, handle);
    let jobs = bounds(&mut cx, handle, "op1-jobs");
    let (at, row) = update(&mut cx, handle, |shell, _, _| {
        let grid = shell.panes[0].as_ref().unwrap().grid.as_ref().unwrap();
        let state = grid.state.borrow();
        let padding = [
            (
                jobs.left() + px(3.),
                jobs.top() + px(3.),
                jobs.left() + px(12.),
                jobs.bottom() - px(3.),
            ),
            (
                jobs.right() - px(12.),
                jobs.top() + px(3.),
                jobs.right() - px(3.),
                jobs.bottom() - px(3.),
            ),
            (
                jobs.left() + px(3.),
                jobs.top() + px(3.),
                jobs.right() - px(3.),
                jobs.top() + px(12.),
            ),
            (
                jobs.left() + px(3.),
                jobs.bottom() - px(12.),
                jobs.right() - px(3.),
                jobs.bottom() - px(3.),
            ),
        ];
        state
            .drawn()
            .iter()
            .filter_map(|tile| {
                let left = state.bounds.left() + px(tile.slot.x);
                let right = left + px(tile.slot.width);
                let top = state.bounds.top() + px(tile.slot.y);
                let bottom = top + px(tile.slot.height);
                padding.iter().find_map(|&(l, t, r, b)| {
                    let x = l.max(left + px(1.));
                    let y = t.max(top + px(1.));
                    (x < r.min(right) && y < b.min(bottom) && tile.listing != 0)
                        .then_some((point(x, y), tile.listing))
                })
            })
            .next()
            .expect("real Grid tile must overlap card padding")
    });
    press(&mut cx, handle, at, MouseButton::Left);
    update(&mut cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!((host.pane.cursor, host.selected.as_slice()), (0, &[0][..]));
    });
    fold(&mut cx, handle);
    press(&mut cx, handle, at, MouseButton::Left);
    update(&mut cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(
            (host.pane.cursor, host.selected.as_slice()),
            (row, &[row][..]),
            "folded control must select the covered real Grid tile"
        );
    });
}
