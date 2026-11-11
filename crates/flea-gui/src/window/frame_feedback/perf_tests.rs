//! Repeatable callback-driven work counts, with wall time separate from the supplied frame schedule.
use super::*;
use crate::window::{root::Root, startup::View};
use crate::{backend::Backend, state::tests::shell_with};
use flea::protocol::{Reply, RowFacts};
use gpui::{
    point, px, size, AppContext, Entity, FrameTerminal, PresentationTime, RequestFrameOptions,
    ScrollDelta, ScrollWheelEvent, TestAppContext, TestDispatcher, VisualTestContext,
};
use std::{cell::Cell, time::Duration};

thread_local! {
    static BUILDS: Cell<[u64; 2]> = const { Cell::new([0; 2]) };
}

pub(crate) fn root_built() {
    BUILDS.with(|builds| {
        let [root, shell] = builds.get();
        builds.set([root + 1, shell]);
    });
}

pub(crate) fn shell_built() {
    BUILDS.with(|builds| {
        let [root, shell] = builds.get();
        builds.set([root, shell + 1]);
    });
}

fn rows(total: usize, count: usize) -> Vec<RowFacts> {
    (0..count.min(total))
        .map(|index| RowFacts {
            name: format!("file-{index:06}.txt"),
            mode: 0o100644,
            size: 26,
            ..Default::default()
        })
        .collect()
}

fn mount(
    cx: &mut TestAppContext,
    total: usize,
) -> (
    Entity<Shell>,
    &mut VisualTestContext,
    impl FnMut() -> Option<String>,
) {
    cx.update(crate::state::constructor_fixture::isolated_services);
    let (backend, requests) = Backend::test_lines();
    let (root, cx) = cx.add_window_view(|window, cx| {
        let shell = cx.new(|cx| shell_with(backend, cx));
        shell.update(cx, |shell, cx| {
            let host = shell.panes[0].as_mut().unwrap();
            host.pane.held = Rc::new(crate::pane::Held {
                listing: 2,
                start: 0,
                total,
                rows: rows(total, 320),
            });
            host.changed(&shell.locale);
            window.focus(&shell.focus, cx);
            cx.set_reduce_motion(true);
        });
        Root { shell }
    });
    cx.enable_frame_feedback();
    let shell = root.read_with(cx, |root, _| root.shell.clone());
    edit(&shell, cx, |shell, window, cx| {
        shell.follow_frame_feedback(window, cx)
    });
    cx.simulate_resize(size(px(1024.), px(899.)));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    callback(cx, Duration::ZERO);
    (shell, cx, requests)
}

fn edit<R>(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    change: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>) -> R,
) -> R {
    cx.update(|window, cx| shell.update(cx, |shell, cx| change(shell, window, cx)))
}

fn callback(cx: &mut VisualTestContext, period: Duration) -> u64 {
    cx.executor().advance_clock(period);
    cx.run_until_parked();
    let before = cx.update(|window, _| window.presented_frames().unwrap().committed);
    cx.simulate_frame_request(RequestFrameOptions::default());
    cx.run_until_parked();
    let after = cx.update(|window, _| window.presented_frames().unwrap().committed);
    if after > before {
        cx.simulate_frame_terminal(FrameTerminal {
            frame_id: after,
            outcome: FrameOutcome::Presented(PresentationTime {
                timestamp: None,
                clock_id: None,
            }),
        });
        cx.run_until_parked();
    }
    after - before
}

fn snapshot(shell: &Entity<Shell>, cx: &VisualTestContext) -> [u64; 4] {
    let [root, shell_builds] = BUILDS.with(Cell::get);
    let (list, snapshots) = shell.read_with(cx, |shell, _| {
        (
            shell.panes[0].as_ref().unwrap().list.borrow().drawn.frames,
            shell.presentation.snapshot_builds.get(),
        )
    });
    [root, shell_builds, list, snapshots]
}

fn sample(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    hz: u64,
    total: usize,
    action: &str,
    batch: usize,
    trial: usize,
) {
    let before = snapshot(shell, cx);
    let start = Instant::now();
    match action {
        "key" => cx.simulate_keystrokes("j"),
        "scroll" => {
            let frame = shell.read_with(cx, |shell, _| {
                shell.panes[0].as_ref().unwrap().input.drawn.unwrap().frame
            });
            let position = point(frame.bounds.left() + px(60.), frame.bounds.top() + px(60.));
            cx.simulate_mouse_move(position, None, gpui::Modifiers::none());
            cx.simulate_event(ScrollWheelEvent {
                position,
                delta: ScrollDelta::Lines(point(0., -1.)),
                ..Default::default()
            });
        }
        "grid" | "columns" | "list" => edit(shell, cx, |shell, window, cx| {
            shell.set_view(
                match action {
                    "grid" => View::Grid,
                    "columns" => View::Columns,
                    _ => View::List,
                },
                window,
                cx,
            );
        }),
        "navigate" => edit(shell, cx, |shell, window, cx| {
            assert!(shell.navigate(0, "/next".into(), None, Default::default(), window, cx));
            let listing = shell.pane(0).unwrap().held.listing + 1;
            let next = rows(total, shell.pane(0).unwrap().first_count());
            for reply in [
                Reply::Listed {
                    path: "/next".into(),
                    total,
                },
                Reply::Rows {
                    start: 0,
                    listing,
                    rows: next,
                },
            ] {
                shell.receive(0, crate::backend::Event::Reply(reply), window, cx);
            }
        }),
        _ => unreachable!(),
    }
    let period = Duration::from_nanos(1_000_000_000 / hz);
    let first = callback(cx, period);
    let app_wall_us = start.elapsed().as_micros();
    let first_work = snapshot(shell, cx);
    let complete = shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.drawn_view, Some(shell.shown_view()));
        let mut complete = true;
        if matches!(action, "key" | "scroll" | "list" | "navigate") {
            let list = host.list.borrow();
            complete = list.drawn_cursor == Some(host.pane.cursor);
            assert!(list.visible().iter().all(|row| row.prepared.is_some()));
            if !complete {
                eprintln!("M9_INCOMPLETE action={action} path={} model={} drawn={:?} scroll={} held={} busy={}", host.path, host.pane.cursor, list.drawn_cursor, host.pane.scroll_y, host.pane.held.listing, host.pane.busy());
            }
        }
        if action == "navigate" {
            assert!(!host.pane.busy());
            assert_eq!(host.path, "/next");
            assert_eq!(shell.path_labels[0].as_ref(), "/next");
        }
        complete
    });
    assert!(
        complete,
        "first callback must contain the completed List state"
    );
    assert_eq!(first, 1, "action must submit its first completed callback");
    let mut followup = 0;
    for _ in 0..3 {
        followup += callback(cx, period);
    }
    let after = snapshot(shell, cx);
    let work = [0, 1, 2, 3].map(|i| first_work[i] - before[i]);
    let tail = [0, 1, 2, 3].map(|i| after[i] - first_work[i]);
    let coverage = if matches!(action, "grid" | "columns") {
        "mounted-view-only"
    } else {
        "list-visible-rows-and-cursor"
    };
    eprintln!("M9_FRAME {{\"batch\":{batch},\"trial\":{trial},\"hz\":{hz},\"total\":{total},\"action\":\"{action}\",\"app_wall_us\":{app_wall_us},\"first_submissions\":{first},\"followup_submissions\":{followup},\"complete\":{complete},\"coverage\":\"{coverage}\",\"builds\":{work:?},\"tail_builds\":{tail:?}}}");
}

fn scrolling(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    hz: u64,
    total: usize,
    batch: usize,
    trial: usize,
) {
    let frame = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().input.drawn.unwrap().frame
    });
    let position = point(frame.bounds.left() + px(60.), frame.bounds.top() + px(60.));
    cx.simulate_mouse_move(position, None, gpui::Modifiers::none());
    callback(cx, Duration::ZERO);
    let mut samples = Vec::new();
    let mut submissions = 0;
    let mut stamps = Vec::new();
    let origin = cx.background_executor.now();
    let before = snapshot(shell, cx);
    for _ in 0..hz / 2 {
        let start = Instant::now();
        cx.simulate_event(ScrollWheelEvent {
            position,
            delta: ScrollDelta::Pixels(point(px(0.), px(-1.))),
            ..Default::default()
        });
        submissions += callback(cx, Duration::from_nanos(1_000_000_000 / hz));
        samples.push(start.elapsed().as_micros());
        stamps.push((
            cx.background_executor
                .now()
                .duration_since(origin)
                .as_nanos(),
            cx.update(|window, _| window.presented_frames().unwrap().committed),
        ));
    }
    let after = snapshot(shell, cx);
    let work = [0, 1, 2, 3].map(|i| after[i] - before[i]);
    assert_eq!(
        submissions,
        hz / 2,
        "one successful submission per scroll callback"
    );
    eprintln!("M9_SCROLL {{\"batch\":{batch},\"trial\":{trial},\"hz\":{hz},\"total\":{total},\"callbacks\":{},\"submissions\":{submissions},\"builds\":{work:?},\"app_wall_us\":{samples:?}}}", hz / 2);
    eprintln!("M9_STAMPS hz={hz} total={total} clock=executor-synthetic native_clock_id=null samples={stamps:?}");
}

fn cached_frames(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    hz: u64,
    total: usize,
    batch: usize,
    trial: usize,
) {
    loading(shell, cx);
    let mut samples = Vec::new();
    let before = snapshot(shell, cx);
    for _ in 0..hz / 2 {
        let start = Instant::now();
        callback(cx, Duration::from_nanos(1_000_000_000 / hz));
        samples.push(start.elapsed().as_micros());
    }
    let after = snapshot(shell, cx);
    let work = [0, 1, 2, 3].map(|i| after[i] - before[i]);
    eprintln!("M9_CACHED {{\"batch\":{batch},\"trial\":{trial},\"hz\":{hz},\"total\":{total},\"callbacks\":{},\"builds\":{work:?},\"app_wall_us\":{samples:?}}}", hz / 2);
}

fn loading(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    edit(shell, cx, |shell, window, cx| {
        cx.set_reduce_motion(false);
        assert!(shell.navigate(0, "/waiting".into(), None, Default::default(), window, cx));
    });
    callback(cx, Duration::from_millis(crate::pane::swap::CAP_MS + 1));
    shell.read_with(cx, |shell, _| assert!(shell.mark_drawn(0)));
}

#[test]
fn m9_perf_frames_baseline_batches() {
    eprintln!(
        "M9_METADATA presentation_bytes={}",
        std::mem::size_of::<Presentation>()
    );
    for batch in 0..2 {
        for trial in 0..5 {
            for hz in [60, 120] {
                for total in [2000, 100_000] {
                    let mut context = TestAppContext::build_with_text_system(
                        TestDispatcher::new(0),
                        None,
                        gpui_platform::current_platform(true).text_system(),
                    );
                    let (shell, cx, _requests) = mount(&mut context, total);
                    for action in ["key", "scroll", "grid", "columns", "list", "navigate"] {
                        sample(&shell, cx, hz, total, action, batch, trial);
                    }
                    scrolling(&shell, cx, hz, total, batch, trial);
                    cached_frames(&shell, cx, hz, total, batch, trial);
                    edit(&shell, cx, |shell, window, cx| shell.close(window, cx));
                }
            }
        }
    }
}

#[path = "perf_gates.rs"]
mod gates;
