use super::*;
use crate::{backend::Backend, state::tests::shell_with, window::root::Root};
use gpui::{AppContext, FrameTerminal, PresentationTime, TestAppContext};
use std::cell::Cell;

thread_local! {
    static WORK: Cell<[usize; 3]> = const { Cell::new([0; 3]) };
}

pub(super) fn work(index: usize) {
    WORK.with(|counter| {
        let mut work = counter.get();
        work[index] += 1;
        counter.set(work);
    });
}

fn presented(id: u64, clock: Option<u32>, ms: Option<u64>) -> FrameTerminal {
    FrameTerminal {
        frame_id: id,
        outcome: FrameOutcome::Presented(PresentationTime {
            timestamp: ms.map(Duration::from_millis),
            clock_id: clock,
        }),
    }
}

#[test]
fn nvidia_hooks_only_complete_submitted_scenes_receive_presentation_timing() {
    let mut frames = Frames::default();
    frames.submitted(1, None);
    frames.terminal(presented(1, Some(1), Some(120)), Some(100_000_000));
    assert_eq!(
        frames.first_ms, None,
        "a loading or incomplete scene is not first complete presentation"
    );
    frames.submitted(2, Some(0.));
    frames.terminal(presented(99, Some(1), Some(130)), Some(100_000_000));
    assert_eq!(
        frames.first_ms, None,
        "unmatched terminal cannot time another submission"
    );
    frames.terminal(presented(2, Some(1), Some(140)), Some(100_000_000));
    assert_eq!(frames.first_ms, Some(40.));
    frames.terminal(presented(2, Some(1), Some(110)), Some(100_000_000));
    assert_eq!(frames.first_ms, Some(40.), "duplicate terminal ignored");
}

#[test]
fn nvidia_hooks_missing_foreign_inverted_and_discarded_times_are_not_first_presentation() {
    let mut frames = Frames {
        scrolling: true,
        end_scroll: Some(0.),
        ..Default::default()
    };
    for (id, clock, ms) in [
        (1, None, Some(120)),
        (2, Some(7), Some(120)),
        (3, Some(1), None),
        (4, Some(1), Some(90)),
    ] {
        frames.submitted(id, Some(0.));
        frames.terminal(presented(id, clock, ms), Some(100_000_000));
    }
    frames.submitted(5, Some(0.));
    frames.terminal(
        FrameTerminal {
            frame_id: 5,
            outcome: FrameOutcome::Discarded,
        },
        Some(100_000_000),
    );
    assert_eq!(frames.first_ms, None);
    assert!(frames.samples.is_empty());
    assert!(frames.pending.is_empty());
    let json = flea::jsondoc::parse(&frames.json(true, true)).unwrap();
    assert_eq!(
        json.get("complete").and_then(flea::jsondoc::Json::as_bool),
        Some(false)
    );
}

#[gpui::test]
fn nvidia_hooks_ready_scene_matches_the_real_submission_and_painted_scroll(
    cx: &mut TestAppContext,
) {
    let (backend, _) = Backend::test_lines();
    let (root, cx) = cx.add_window_view(|_, cx| Root {
        shell: cx.new(|cx| shell_with(backend, cx)),
    });
    let shell = root.read_with(cx, |root, _| root.shell.clone());
    cx.enable_frame_feedback();
    let scenes = Rc::new(RefCell::new(Vec::new()));
    let captured = scenes.clone();
    let weak = shell.downgrade();
    cx.update(|window, cx| {
        cx.set_reduce_motion(true);
        shell.update(cx, |shell, cx| {
            shell.follow_frame_feedback(window, cx);
            let host = shell.panes[0].as_mut().unwrap();
            let held = &host.pane.held;
            host.pane.held = Rc::new(crate::pane::Held {
                listing: held.listing,
                total: 10_000,
                start: held.start,
                rows: held.rows.clone(),
            });
            host.changed(&shell.locale);
            cx.notify();
        });
        window
            .observe_frame_feedback(cx, move |event, _, cx| {
                if let FrameFeedbackEvent::Submitted { draw_id, frame_id } = event {
                    let scene = weak
                        .read_with(cx, |shell, _| {
                            shell.presentation.gpu_report_scene(draw_id, frame_id)
                        })
                        .unwrap();
                    captured.borrow_mut().push(scene);
                }
            })
            .unwrap()
            .detach();
    });
    let submit = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, _| window.refresh());
        cx.simulate_frame_request(gpui::RequestFrameOptions {
            require_presentation: true,
            ..Default::default()
        });
        cx.run_until_parked();
    };
    submit(cx);
    assert!(
        scenes.borrow().iter().any(Option::is_some),
        "real complete List scene is eligible"
    );
    scenes.borrow_mut().clear();
    cx.update(|_, cx| {
        shell.update(cx, |shell, cx| {
            shell.panes[0].as_mut().unwrap().pane.state.kind = crate::pane::states::Kind::Loading;
            cx.notify();
        })
    });
    submit(cx);
    assert!(!scenes.borrow().is_empty());
    assert!(
        scenes.borrow().iter().all(Option::is_none),
        "loading draws must not become complete timing"
    );
    scenes.borrow_mut().clear();
    cx.update(|_, cx| {
        shell.update(cx, |shell, cx| {
            let host = shell.panes[0].as_mut().unwrap();
            host.pane.state.kind = crate::pane::states::Kind::Ready;
            host.pane.scroll_to(300.);
            cx.notify();
        })
    });
    submit(cx);
    assert!(
        scenes.borrow().contains(&Some(300.)),
        "receipt reads the painted List scroll, not the requested driver offset"
    );
}

#[test]
fn nvidia_hooks_scroll_intervals_use_presentation_order_and_painted_movement() {
    let mut frames = Frames {
        scrolling: true,
        ..Default::default()
    };
    for id in 1..=4 {
        frames.submitted(id, Some((id - 1) as f32 * 5000.));
    }
    for (id, ms) in [(3, 3400), (1, 100), (4, 5100), (2, 1700)] {
        frames.terminal(presented(id, Some(1), Some(ms)), Some(50_000_000));
    }
    let json = flea::jsondoc::parse(&frames.json(true, true)).unwrap();
    assert_eq!(
        json.get("first_ms").and_then(flea::jsondoc::Json::as_f64),
        Some(50.)
    );
    assert_eq!(
        json.get("scroll_px").and_then(flea::jsondoc::Json::as_f64),
        Some(15000.)
    );
    let intervals: Vec<_> = json
        .get("intervals_ms")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_f64().unwrap())
        .collect();
    assert_eq!(intervals, [1600., 1700., 1700.]);
    for (_, scroll) in &mut frames.samples {
        *scroll = 0.;
    }
    let still = flea::jsondoc::parse(&frames.json(true, true)).unwrap();
    assert!(
        still
            .get("intervals_ms")
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty(),
        "redrawing a stationary listing is not scrolling"
    );
}

#[test]
fn nvidia_hooks_review_pending_scroll_tail_cannot_report_complete() {
    let mut frames = Frames {
        scrolling: true,
        end_scroll: Some(3000.),
        ..Default::default()
    };
    frames.submitted(1, Some(1000.));
    frames.submitted(2, Some(3000.));
    frames.scrolling = false;
    let timed_out = flea::jsondoc::parse(&frames.json(true, true)).unwrap();
    assert_eq!(
        timed_out
            .get("complete")
            .and_then(flea::jsondoc::Json::as_bool),
        Some(false)
    );
    frames.terminal(presented(2, Some(1), Some(5100)), None);
    let waiting = flea::jsondoc::parse(&frames.json(true, true)).unwrap();
    assert_eq!(
        waiting
            .get("complete")
            .and_then(flea::jsondoc::Json::as_bool),
        Some(false),
        "the final frame does not settle earlier outstanding scroll receipts"
    );
    frames.terminal(presented(1, Some(1), Some(5090)), None);
    let drained = flea::jsondoc::parse(&frames.json(true, true)).unwrap();
    assert_eq!(
        drained
            .get("complete")
            .and_then(flea::jsondoc::Json::as_bool),
        Some(true)
    );
}

#[test]
fn nvidia_hooks_review_repeated_scroll_positions_preserve_the_visible_stall() {
    let mut frames = Frames {
        scrolling: true,
        ..Default::default()
    };
    for (id, ms, scroll) in [(1, 100, 0.), (2, 116, 0.), (3, 132, 96.), (4, 148, 96.)] {
        frames.submitted(id, Some(scroll));
        frames.terminal(presented(id, Some(1), Some(ms)), None);
    }
    let json = flea::jsondoc::parse(&frames.json(true, true)).unwrap();
    let intervals: Vec<_> = json
        .get("intervals_ms")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_f64().unwrap())
        .collect();
    assert_eq!(intervals, [32., 16.], "stationary repaint cannot split the time until visible movement; trailing stall remains timed");
}

#[test]
fn nvidia_hooks_scroll_samples_are_scoped_to_submission_time_and_bounded() {
    let mut frames = Frames::default();
    frames.submitted(1, Some(0.));
    frames.scrolling = true;
    frames.terminal(presented(1, Some(1), Some(100)), None);
    assert!(
        frames.samples.is_empty(),
        "pre-scroll submission is not a scroll sample"
    );
    for id in 2..=700 {
        frames.submitted(id, Some(id as f32));
    }
    assert_eq!(frames.pending.len(), RETAINED);
    frames.terminal(presented(2, Some(1), Some(110)), None);
    assert!(
        frames.samples.is_empty(),
        "evicted receipt cannot time another frame"
    );
    frames.scrolling = false;
    frames.terminal(presented(700, Some(1), Some(120)), None);
    assert_eq!(
        frames.samples.len(),
        1,
        "delayed scroll terminal stays a scroll sample"
    );
    for id in 701..=6000 {
        frames.scrolling = true;
        frames.submitted(id, Some(id as f32));
        frames.terminal(presented(id, Some(1), Some(id)), None);
    }
    assert_eq!(frames.samples.len(), SAMPLES);
    frames.pending.clear();
    let json = flea::jsondoc::parse(&frames.json(true, true)).unwrap();
    assert_eq!(
        json.get("complete").and_then(flea::jsondoc::Json::as_bool),
        Some(false),
        "bounded retention must report lost coverage rather than success"
    );
}

#[gpui::test]
fn nvidia_hooks_disabled_has_zero_report_state_and_zero_frame_work(cx: &mut TestAppContext) {
    WORK.with(|counter| counter.set([0; 3]));
    let (backend, _) = Backend::test_lines();
    let (root, cx) = cx.add_window_view(|_, cx| Root {
        shell: cx.new(|cx| shell_with(backend, cx)),
    });
    cx.enable_frame_feedback();
    let shell = root.read_with(cx, |root, _| root.shell.clone());
    assert!(
        !enabled(),
        "disabled-cost control runs with report disabled"
    );
    cx.update(|window, cx| start(&shell, window, cx));
    for _ in 0..30 {
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            window.refresh();
            window.draw(cx).clear(cx);
        });
        cx.run_until_parked();
    }
    assert_eq!(WORK.with(Cell::get), [0; 3]);
    cx.update(|window, cx| start_if(true, &shell, window, cx));
    cx.update(|window, cx| {
        window.simulate_next_frame(cx);
    });
    let observed = WORK.with(Cell::get);
    assert_eq!(observed[0], 1, "positive control creates report state");
    assert!(observed[2] > 0, "positive control runs report frame driver");
    println!("nvidia_hooks disabled after 30 draws: report states=0, feedback callbacks=0, frame-driver callbacks=0");
}

#[test]
fn nvidia_hooks_review_elapsed_scroll_requires_the_final_presentation() {
    let mut frames = Frames {
        scrolling: true,
        ..Default::default()
    };
    for (id, ms, scroll) in [(1, 1000, 0.), (2, 3250, 6750.), (3, 5500, 13500.)] {
        frames.submitted(id, Some(scroll));
        frames.terminal(presented(id, Some(1), Some(ms)), Some(1_000_000_000));
    }
    frames.scrolling = false;
    let json = flea::jsondoc::parse(&frames.json(true, true)).unwrap();
    assert_eq!(
        json.get("complete").and_then(flea::jsondoc::Json::as_bool),
        Some(false),
        "elapsed scroll and drained older frames cannot acknowledge an unsubmitted final position"
    );
}
