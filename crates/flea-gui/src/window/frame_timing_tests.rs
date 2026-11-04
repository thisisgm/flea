//! The unfocused-window scene's counters, GPU readiness labelling and compositor presentation timing.
use super::fixture::*;
use flea::jsondoc::Json;
use gpui::{FrameOutcome, PresentationTime, RequestFrameOptions};
use std::{cell::Cell, rc::Rc, time::Duration};

const MONOTONIC: u32 = 1;

fn scene(shell: &gpui::Entity<crate::state::Shell>, cx: &mut gpui::VisualTestContext) -> Json {
    cx.update(|window, cx| shell.read(cx).visual_state(window))
}

fn count(json: &Json, object: &str, field: &str) -> u64 {
    match json.get(object).and_then(|value| value.get(field)) {
        Some(Json::Num(value)) => value.parse().unwrap(),
        other => panic!("{object}.{field} must be a count, got {other:?}"),
    }
}

/// A frame callback that presents only what is dirty, as the compositor's idle callbacks do.
fn idle_frame(cx: &mut gpui::VisualTestContext) {
    cx.simulate_frame_request(RequestFrameOptions::default());
    cx.run_until_parked();
}

fn settled(shell: &gpui::Entity<crate::state::Shell>, cx: &mut gpui::VisualTestContext) {
    ready_rows(shell, cx, "/files", 1);
    for _ in 0..4 {
        step(cx, Duration::from_millis(250));
        idle_frame(cx);
    }
}

#[gpui::test]
fn unfocused_window_at_rest_requests_no_frames_and_notifies_nothing(cx: &mut gpui::TestAppContext) {
    let (shell, cx, _requests) = mount(cx, true);
    settled(&shell, cx);
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    idle_frame(cx);
    assert_eq!(
        scene(&shell, cx)
            .get("activity")
            .and_then(|a| a.get("active")),
        Some(&Json::Bool(true))
    );
    cx.deactivate_window();
    // The blur itself may repaint the unfocused ink once; the scene samples after it settles.
    for _ in 0..4 {
        step(cx, Duration::from_millis(250));
        idle_frame(cx);
    }
    let before = scene(&shell, cx);
    assert_eq!(
        before.get("activity").and_then(|a| a.get("active")),
        Some(&Json::Bool(false))
    );
    for _ in 0..100 {
        step(cx, Duration::from_millis(100));
        idle_frame(cx);
    }
    let after = scene(&shell, cx);
    for field in ["draws", "notifications", "committed"] {
        assert_eq!(
            count(&after, "activity", field),
            count(&before, "activity", field),
            "an unfocused window at rest changed {field} over 10 s"
        );
    }
    // Positive control: the counters are live, so the zero above is a measurement.
    shell.update(cx, |_, cx| cx.notify());
    idle_frame(cx);
    let poked = scene(&shell, cx);
    assert_eq!(
        count(&poked, "activity", "notifications"),
        count(&after, "activity", "notifications") + 1
    );
    assert!(count(&poked, "activity", "draws") > count(&after, "activity", "draws"));
    assert!(count(&poked, "activity", "committed") > count(&after, "activity", "committed"));
}

#[gpui::test]
fn gpu_readiness_is_never_taken_from_submission_or_presentation(cx: &mut gpui::TestAppContext) {
    let (shell, cx, _requests) = mount(cx, false);
    ready_rows(&shell, cx, "/files", 1);
    let frame = submit(&shell, cx);
    terminal(
        cx,
        &frame,
        FrameOutcome::Presented(PresentationTime {
            timestamp: Some(Duration::from_secs(5)),
            clock_id: Some(MONOTONIC),
        }),
    );
    let gpu = scene(&shell, cx)
        .get("presentation")
        .and_then(|p| p.get("gpuReadiness"))
        .cloned()
        .expect("presentation names its GPU readiness coverage");
    assert_eq!(
        gpu.get("status").and_then(Json::as_str),
        Some("unavailable")
    );
    assert_eq!(gpu.get("readyMs"), Some(&Json::Null));
    assert_eq!(
        gpu.get("reason").and_then(Json::as_str),
        Some("gpu-completion-not-exposed")
    );
}

fn presented_at(ms: f64, clock_id: Option<u32>) -> FrameOutcome {
    FrameOutcome::Presented(PresentationTime {
        timestamp: Some(Duration::from_secs_f64(ms / 1000.)),
        clock_id,
    })
}

fn series(json: &Json, name: &str) -> Option<[f64; 5]> {
    let value = json.get("presentation")?.get("presentTiming")?.get(name)?;
    if value == &Json::Null {
        return None;
    }
    let field = |key: &str| match value.get(key) {
        Some(Json::Num(n)) => n.parse::<f64>().unwrap(),
        other => panic!("{name}.{key} must be a number, got {other:?}"),
    };
    Some([
        field("count"),
        field("p50"),
        field("p95"),
        field("p99"),
        field("max"),
    ])
}

#[gpui::test]
fn commit_to_present_reads_only_monotonic_compositor_timestamps(cx: &mut gpui::TestAppContext) {
    let (shell, cx, _requests) = mount(cx, false);
    ready_rows(&shell, cx, "/files", 1);
    let now = Rc::new(Cell::new(Duration::from_millis(1000)));
    let clock = now.clone();
    shell.update(cx, |shell, _| {
        shell
            .presentation
            .timing
            .set_clock(Rc::new(move || clock.get()))
    });
    let timing = |cx: &mut gpui::VisualTestContext| {
        scene(&shell, cx)
            .get("presentation")
            .and_then(|p| p.get("presentTiming"))
            .cloned()
            .unwrap()
    };
    let before = scene(&shell, cx);
    assert_eq!(series(&before, "commitToPresentMs"), None, "no sample yet");
    assert_eq!(
        timing(cx).get("clock").and_then(Json::as_str),
        Some("CLOCK_MONOTONIC")
    );
    assert_eq!(
        timing(cx).get("start").and_then(Json::as_str),
        Some("submit-observed")
    );

    let first = submit(&shell, cx);
    terminal(cx, &first, presented_at(1008., Some(MONOTONIC)));
    now.set(Duration::from_millis(1010));
    shell.update(cx, |_, cx| cx.notify());
    let second = submit(&shell, cx);
    terminal(cx, &second, presented_at(1016., Some(MONOTONIC)));
    // A foreign clock, a missing timestamp and a discard never become samples.
    shell.update(cx, |_, cx| cx.notify());
    let foreign = submit(&shell, cx);
    terminal(cx, &foreign, presented_at(1024., Some(4)));
    shell.update(cx, |_, cx| cx.notify());
    let untimed = submit(&shell, cx);
    terminal(cx, &untimed, presented_at(1032., None));
    shell.update(cx, |_, cx| cx.notify());
    let dropped = submit(&shell, cx);
    terminal(cx, &dropped, FrameOutcome::Discarded);
    // The next timed present follows the gap, so it gives a latency but never an interval.
    now.set(Duration::from_millis(1035));
    shell.update(cx, |_, cx| cx.notify());
    let after_gap = submit(&shell, cx);
    terminal(cx, &after_gap, presented_at(1040., Some(MONOTONIC)));

    let json = scene(&shell, cx);
    assert_eq!(
        series(&json, "commitToPresentMs"),
        Some([3., 6., 8., 8., 8.])
    );
    assert_eq!(
        series(&json, "presentIntervalMs"),
        Some([1., 8., 8., 8., 8.])
    );
    let timing = timing(cx);
    assert_eq!(timing.get("foreignClock"), Some(&Json::Num("1".into())));
    assert_eq!(timing.get("untimed"), Some(&Json::Num("1".into())));
}

#[gpui::test]
fn a_presentation_before_its_submission_is_refused_not_clamped(cx: &mut gpui::TestAppContext) {
    let (shell, cx, _requests) = mount(cx, false);
    ready_rows(&shell, cx, "/files", 1);
    shell.update(cx, |shell, _| {
        shell
            .presentation
            .timing
            .set_clock(Rc::new(|| Duration::from_millis(2000)))
    });
    let frame = submit(&shell, cx);
    terminal(cx, &frame, presented_at(1990., Some(MONOTONIC)));
    let json = scene(&shell, cx);
    assert_eq!(series(&json, "commitToPresentMs"), None);
    assert_eq!(
        json.get("presentation")
            .and_then(|p| p.get("presentTiming"))
            .and_then(|t| t.get("inverted")),
        Some(&Json::Num("1".into()))
    );
}
