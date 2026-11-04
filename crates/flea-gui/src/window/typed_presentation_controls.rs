//! Negative paths use actual Window receipt IDs, never manually inserted associations.
use super::fixture::*;
use gpui::TestAppContext;
use std::time::Duration;

#[gpui::test]
fn typed_paint_skip_and_successful_submit_do_not_ack_before_presented(cx: &mut TestAppContext) {
    let (shell, cx, _requests) = mount(cx, false);
    navigate(&shell, cx, "/next", false);
    step(cx, Duration::from_millis(66));
    let before = cx.update(|window, cx| {
        let before = window.presented_frames().unwrap();
        window.draw(cx).clear(cx);
        assert_eq!(
            window.presented_frames(),
            Some(before),
            "Root paint is not a backend receipt"
        );
        before.committed
    });
    shell.read_with(cx, |shell, _| {
        assert_eq!(shell.pane(0).unwrap().swap.mark_since, None);
        assert_eq!(shell.pane(0).unwrap().swap.observed.borrow().frames, 0);
    });
    cx.skip_next_present();
    cx.simulate_frame_request(gpui::RequestFrameOptions {
        require_presentation: true,
        ..Default::default()
    });
    cx.run_until_parked();
    assert_eq!(
        cx.update(|window, _| window.presented_frames().unwrap().committed),
        before
    );
    let loading = submit(&shell, cx);
    ready_rows(&shell, cx, "/next", 3);
    whole_loading(&shell, cx, &loading);
    assert_eq!(
        loading.observed.borrow().frames,
        0,
        "paints and submissions cannot count as presented"
    );
    assert_eq!(loading.observed.borrow().loading, 0);
    assert_eq!(loading.observed.borrow().outstanding_loading, 1);
    assert!(shell.read_with(cx, |shell, _| shell
        .pane(0)
        .unwrap()
        .swap
        .mark_since
        .is_none()));
    step(cx, Duration::from_millis(200));
    let observer_ms = shell.read_with(cx, |shell, _| shell.pane(0).unwrap().now());
    terminal(cx, &loading, presented());
    assert_eq!(loading.observed.borrow().frames, 1);
    shell.read_with(cx, |shell, _| {
        assert_eq!(shell.pane(0).unwrap().swap.mark_since, Some(observer_ms))
    });
    step(cx, Duration::from_millis(99));
    let still_loading = submit(&shell, cx);
    whole_loading(&shell, cx, &still_loading);
    step(cx, Duration::from_millis(1));
    let ready = submit(&shell, cx);
    whole_ready(&shell, cx, &ready, "/next");
    terminal(cx, &ready, presented());
    close(&shell, cx);
}

#[gpui::test]
fn typed_superseded_loading_receipt_cannot_ack_new_generation(cx: &mut TestAppContext) {
    let (shell, cx, _requests) = mount(cx, false);
    let old = loading(&shell, cx, &[Duration::from_millis(33)]);
    let old_count = old.observed.borrow().frames;
    navigate(&shell, cx, "/retarget", true);
    step(cx, Duration::from_millis(66));
    let new = submit(&shell, cx);
    assert!(new.loading);
    assert_ne!(new.content.generation, old.content.generation);
    assert!(!std::rc::Rc::ptr_eq(&new.observed, &old.observed));
    terminal(cx, &old, presented());
    assert_eq!(old.observed.borrow().frames, old_count + 1);
    assert_eq!(new.observed.borrow().frames, 0);
    shell.read_with(cx, |shell, _| {
        assert!(shell.mark_drawn(0));
        assert_eq!(
            shell.pane(0).unwrap().swap.mark_since,
            None,
            "old generation may not acknowledge current Loading"
        );
    });
    ready_rows(&shell, cx, "/retarget", 4);
    terminal(cx, &new, presented());
    let acknowledged = shell.read_with(cx, |shell, _| shell.pane(0).unwrap().swap.mark_since);
    assert!(acknowledged.is_some());
    step(cx, Duration::from_millis(99));
    let still_loading = submit(&shell, cx);
    whole_loading(&shell, cx, &still_loading);
    step(cx, Duration::from_millis(1));
    let ready = submit(&shell, cx);
    whole_ready(&shell, cx, &ready, "/retarget");
    terminal(cx, &ready, presented());
    close(&shell, cx);
}

#[gpui::test]
fn typed_replay_out_of_order_and_duplicates_preserve_counts_and_metadata(cx: &mut TestAppContext) {
    let (shell, cx, _requests) = mount(cx, false);
    let a = loading(&shell, cx, &[Duration::from_millis(33)]);
    let b = submit(&shell, cx);
    let baseline = a.observed.borrow().frames;
    assert_ne!(a.id, b.id);
    assert_eq!(
        a.draw, b.draw,
        "clean Window submission reuses its retained scene"
    );
    assert!(std::rc::Rc::ptr_eq(&a.content, &b.content));
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().status = "later model status".into();
    });
    terminal(cx, &b, presented());
    terminal(cx, &a, presented());
    terminal(cx, &b, presented());
    assert_eq!(a.observed.borrow().frames, baseline + 2);
    assert_eq!(a.observed.borrow().last_id, b.id);
    assert_ne!(
        a.observed.borrow().last.as_ref().unwrap().status.as_deref(),
        Some("later model status")
    );
    assert_eq!(
        cx.update(|window, _| window.presented_frames().unwrap().presented),
        b.id
    );
    unknowns(&a.observed.borrow());
    close(&shell, cx);
}

#[gpui::test]
fn typed_minimum_uses_observer_clock_separately_from_optional_native_time(cx: &mut TestAppContext) {
    let (shell, cx, _requests) = mount(cx, false);
    let loading = loading(&shell, cx, &[Duration::from_millis(33)]);
    ready_rows(&shell, cx, "/next", 3);
    step(cx, Duration::from_millis(200));
    let observer = shell.read_with(cx, |shell, _| shell.pane(0).unwrap().now());
    // Deliberately incomparable test payload, not a validated native latency clock.
    terminal(
        cx,
        &loading,
        gpui::FrameOutcome::Presented(gpui::PresentationTime {
            timestamp: Some(Duration::from_secs(42)),
            clock_id: None,
        }),
    );
    assert_eq!(
        loading.observed.borrow().native_at,
        Some(Duration::from_secs(42))
    );
    assert_eq!(loading.observed.borrow().clock_id, None);
    assert_eq!(loading.observed.borrow().observer_ms, Some(observer));
    let json = loading.observed.borrow().json();
    assert_eq!(
        json.get("minimumClock")
            .and_then(flea::jsondoc::Json::as_str),
        Some("app-observer")
    );
    shell.read_with(cx, |shell, _| {
        assert_eq!(shell.pane(0).unwrap().swap.mark_since, Some(observer))
    });
    step(cx, Duration::from_millis(99));
    assert!(shell.read_with(cx, |shell, _| shell.pane(0).unwrap().busy()));
    step(cx, Duration::from_millis(1));
    let ready = submit(&shell, cx);
    terminal(cx, &ready, presented());
    whole_ready(&shell, cx, &ready, "/next");
    close(&shell, cx);
}
