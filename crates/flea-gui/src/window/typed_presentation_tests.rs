//! Authoring only until the controller stages the reviewed terminal dependency.
#[path = "typed_presentation_controls.rs"]
mod controls;
#[path = "typed_presentation_fixture.rs"]
mod fixture;
#[path = "frame_timing_tests.rs"]
mod timing;
use fixture::*;
use gpui::{FrameOutcome, TestAppContext};
use std::time::Duration;

#[gpui::test]
fn typed_presented_starts_minimum_at_observer_arrival_across_cadences(cx: &mut TestAppContext) {
    for schedule in schedules() {
        let (shell, cx, _requests) = mount(cx, false);
        let first = loading(&shell, cx, &schedule);
        let baseline = first.observed.borrow().frames;
        step(cx, Duration::from_millis(5));
        ready_rows(&shell, cx, "/next", 3);
        let ack_at = first.at + Duration::from_millis(200);
        let mut next = cx.executor().now();
        for period in schedule.iter().cycle() {
            next += *period;
            if next >= ack_at {
                break;
            }
            until(cx, next);
            let frame = submit(&shell, cx);
            whole_loading(&shell, cx, &frame);
            assert_eq!(
                frame.observed.borrow().frames,
                baseline,
                "submission is not presentation"
            );
            assert!(shell.read_with(cx, |shell, _| shell
                .pane(0)
                .unwrap()
                .swap
                .mark_since
                .is_none()));
        }
        until(cx, ack_at);
        let observer_ms = shell.read_with(cx, |shell, _| shell.pane(0).unwrap().now());
        terminal(cx, &first, presented());
        assert_eq!(first.observed.borrow().frames, baseline + 1);
        assert_eq!(first.observed.borrow().observer_ms, Some(observer_ms));
        shell.read_with(cx, |shell, _| {
            assert_eq!(shell.pane(0).unwrap().swap.mark_since, Some(observer_ms))
        });
        step(cx, Duration::from_millis(99));
        let still_loading = submit(&shell, cx);
        whole_loading(&shell, cx, &still_loading);
        terminal(cx, &still_loading, presented());
        shell.read_with(cx, |shell, _| {
            assert_eq!(shell.pane(0).unwrap().swap.mark_since, Some(observer_ms))
        });
        step(cx, Duration::from_millis(1));
        let ready = submit(&shell, cx);
        whole_ready(&shell, cx, &ready, "/next");
        terminal(cx, &ready, presented());
        shell.read_with(cx, |shell, _| {
            assert_eq!(shell.pane(0).unwrap().swap.counters.shown_ms, 100)
        });
        assert_eq!(ready.observed.borrow().last_id, ready.id);
        close(&shell, cx);
    }
}

#[gpui::test]
fn typed_discarded_loading_releases_ready_pair_across_cadences(cx: &mut TestAppContext) {
    for schedule in schedules() {
        let (shell, cx, _requests) = mount(cx, false);
        let first = loading(&shell, cx, &schedule);
        let baseline = first.observed.borrow().frames;
        ready_rows(&shell, cx, "/next", 3);
        assert!(shell.read_with(cx, |shell, _| shell.pane(0).unwrap().busy()));
        terminal(cx, &first, FrameOutcome::Discarded);
        assert_eq!(first.observed.borrow().frames, baseline);
        assert_eq!(first.observed.borrow().discarded, 1);
        assert_eq!(first.observed.borrow().unavailable, 0);
        shell.read_with(cx, |shell, _| {
            assert_eq!(shell.pane(0).unwrap().swap.mark_since, None)
        });
        let ready = submit(&shell, cx);
        whole_ready(&shell, cx, &ready, "/next");
        terminal(cx, &ready, presented());
        close(&shell, cx);
    }
}

#[gpui::test]
fn typed_silent_animation_has_bounded_deadline_across_cadences_and_stall(cx: &mut TestAppContext) {
    for schedule in schedules() {
        let (shell, cx, _requests) = mount(cx, true);
        let first = loading(&shell, cx, &schedule);
        let baseline = first.observed.borrow().frames;
        let deadline = first.at + Duration::from_millis(1500);
        ready_rows(&shell, cx, "/next", 3);
        let mut next = first.at;
        for period in schedule.iter().cycle() {
            next += *period;
            if next >= deadline - Duration::from_millis(1) {
                break;
            }
            until(cx, next);
            let frame = submit(&shell, cx);
            whole_loading(&shell, cx, &frame);
            assert_eq!(frame.observed.borrow().frames, baseline);
            assert_eq!(frame.observed.borrow().discarded, 0);
            assert!(!frame.observed.borrow().wait_unavailable);
        }
        until(cx, deadline - Duration::from_millis(1));
        assert!(shell.read_with(cx, |shell, _| shell.pane(0).unwrap().busy()));
        until(cx, deadline);
        assert!(
            !shell.read_with(cx, |shell, _| shell.pane(0).unwrap().busy()),
            "continued animation must not extend the generation wait"
        );
        assert!(first.observed.borrow().wait_unavailable);
        assert!(first.observed.borrow().unavailable > 0);
        assert_eq!(first.observed.borrow().discarded, 0);
        assert_eq!(first.observed.borrow().frames, baseline);
        shell.read_with(cx, |shell, _| {
            assert!(shell.pane(0).unwrap().swap.mark.counters.requests.get() > 1)
        });
        until(cx, first.at + Duration::from_millis(2500));
        let ready = submit(&shell, cx);
        whole_ready(&shell, cx, &ready, "/next");
        terminal(cx, &first, presented());
        assert_eq!(
            first.observed.borrow().frames,
            baseline,
            "retired metadata cannot count a late terminal"
        );
        shell.read_with(cx, |shell, _| {
            assert_eq!(shell.pane(0).unwrap().swap.mark_since, None);
            assert!(shell.presentation.unmatched_terminals > 0);
        });
        terminal(cx, &ready, presented());
        assert_eq!(first.observed.borrow().frames, baseline + 1);
        assert_eq!(first.observed.borrow().discarded, 0);
        close(&shell, cx);
    }
}

#[gpui::test]
fn typed_omitted_feedback_survives_a_single_2500ms_executor_stall(cx: &mut TestAppContext) {
    let (shell, cx, _requests) = mount(cx, false);
    let first = loading(&shell, cx, &[Duration::from_millis(33)]);
    let baseline = first.observed.borrow().frames;
    ready_rows(&shell, cx, "/next", 3);
    step(cx, Duration::from_millis(2500));
    let ready = submit(&shell, cx);
    whole_ready(&shell, cx, &ready, "/next");
    assert_eq!(first.observed.borrow().frames, baseline);
    assert_eq!(first.observed.borrow().discarded, 0);
    assert_eq!(first.observed.borrow().unavailable, 1);
    assert!(first.observed.borrow().wait_unavailable);
    terminal(cx, &first, FrameOutcome::Discarded);
    assert_eq!(
        first.observed.borrow().discarded,
        0,
        "retirement is not a synthesized discard"
    );
    terminal(cx, &ready, presented());
    close(&shell, cx);
}
