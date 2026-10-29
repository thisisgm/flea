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

/// The track a wheel reaches in this view: the Columns Active track or the List, as drawn.
fn track(cx: &mut TestAppContext, handle: Handle, view: View) -> Bounds<Pixels> {
    update(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        let bounds = if view == View::Columns {
            let paint = host
                .columns
                .as_ref()
                .unwrap()
                .painters
                .get(Slot::Active)
                .unwrap()
                .borrow();
            paint.drawn.as_ref().unwrap().bounds
        } else {
            host.list.borrow().drawn.bounds
        };
        bounds
    })
}

fn moving(cx: &mut TestAppContext, handle: Handle, view: View) -> bool {
    update(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        if view == View::Columns {
            host.columns.as_ref().unwrap().scroll[Slot::Active.index()]
                .input
                .moving()
        } else {
            host.input.moving()
        }
    })
}

/// Hands one wheel to the view's own handler; an open menu or card occludes the track at the window level.
fn wheel_handler(cx: &mut TestAppContext, handle: Handle, view: View, at: Point<Pixels>) {
    let event = ScrollWheelEvent {
        position: at,
        delta: ScrollDelta::Lines(point(0., -3.)),
        modifiers: Modifiers::none(),
        touch_phase: gpui::TouchPhase::Moved,
    };
    update(cx, handle, |shell, window, cx| {
        if view == View::Columns {
            shell.columns_scroll_wheel(0, &event, cx)
        } else {
            shell.pointer_wheel(0, &event, window, cx)
        }
    });
}

#[test]
fn wheel_under_an_open_menu_leaves_the_track_still() {
    for view in [View::List, View::Columns] {
        let (mut cx, handle, mut received) = open(view);
        reset(&mut cx, handle);
        let track = track(&mut cx, handle, view);
        let press_at = track.center();
        press(&mut cx, handle, press_at, MouseButton::Right);
        assert!(
            received().is_some_and(|line| line.contains("\"op\":\"snapshot\"")),
            "{view:?}: a right press asks for the menu snapshot"
        );
        // The backend answers the snapshot and the applications list, as the menu tests do.
        update(&mut cx, handle, |shell, window, cx| {
            let captured = shell.menu.captured.as_ref().expect("a captured menu");
            let (side, id) = (captured.side, captured.id);
            shell.receive(
                side,
                Event::Producer(format!(
                    r#"{{"t":"menuaction","id":{id},"op":"snapshot","ok":true,"kinds":[]}}"#
                )),
                window,
                cx,
            );
            shell.receive(
                side,
                Event::Producer(format!(
                    r#"{{"t":"menuaction","id":{id},"op":"applications","ok":true,"applications":[]}}"#
                )),
                window,
                cx,
            );
        });
        assert!(
            update(&mut cx, handle, |shell, _, _| shell.menu_opened()),
            "{view:?}: the answered snapshot opens the menu"
        );
        let at = point(track.left() + px(5.), track.top() + px(30.));
        wheel_handler(&mut cx, handle, view, at);
        assert_eq!(
            scroll(&mut cx, handle),
            0.,
            "{view:?}: a wheel under an open menu moved the track"
        );
    }
}

#[path = "jobs_track_input_tests.rs"]
mod extracted;

#[path = "jobs_modal_tests.rs"]
mod modal;
