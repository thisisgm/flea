//! T01: the protocol's literal 15-second boundary is independent of its implementation constant.
use super::{test_support::World, transfer};

const FINISHED: u64 = 100_000;

#[test]
fn sol_recent_tabs_remainder_t01_ack_at_15000_closes_only_the_captured_tab_once_after_busy() {
    let mut source = World::new(3);
    let mut bar = source.bar();
    let captured = source.identities()[1];
    bar.tab_lift_began(&mut source, 1, 7);
    let token = bar.out_token.clone();
    let payload = bar.out_payload.clone();
    bar.out_finished(FINISHED);
    source.tabs.move_tab(&mut source.pane, 1, 0);
    source.tabs.select_at(&mut source.pane, 1);
    source.pane.busy = true;
    let before = source.identities();
    assert!(!bar.take(&mut source, "wrong", FINISHED + 15_000));
    let mut destination = World::new(1);
    let mut receiver = destination.bar();
    receiver.self_pid = "222".into();
    let info = transfer::parse(&payload).unwrap();
    assert!(receiver.accept_tab_drop(&mut destination, &payload, &info, None, 110_000));
    receiver.on_peeked(
        &mut destination,
        0,
        &info.path,
        false,
        false,
        false,
        transfer::PEEK_FIRST,
    );
    let (pid, acknowledged) = receiver
        .pump_taken()
        .expect("accepted receiver sends taken");
    assert_eq!(pid, "111");
    assert_eq!(acknowledged, token);
    assert!(
        bar.take(&mut source, &acknowledged, FINISHED + 15_000),
        "15000 ms after drag finish is valid"
    );
    assert_eq!(source.identities(), before, "a busy source defers closure");
    bar.drain_lifts(&mut source, 200_000);
    assert_eq!(
        source.identities(),
        before,
        "an accepted ack survives expiry while busy"
    );
    source.pane.busy = false;
    bar.drain_lifts(&mut source, 200_001);
    let kept: Vec<_> = before
        .into_iter()
        .filter(|identity| *identity != captured)
        .collect();
    assert_eq!(
        source.identities(),
        kept,
        "only the captured identity closes after reorder and focus change"
    );
    assert_eq!(destination.tabs.count(), 2);
    assert!(!bar.take(&mut source, &acknowledged, 200_002));
    bar.drain_lifts(&mut source, 200_003);
    assert_eq!(
        source.identities(),
        kept,
        "the same ack never closes a second tab"
    );
    assert!(bar.lifts.is_empty());
    assert_eq!(receiver.taken_exited(), None);
}

#[test]
fn sol_recent_tabs_remainder_t01_missing_and_late_ack_keep_source_at_15001() {
    for drain_first in [false, true] {
        let mut source = World::new(2);
        let mut bar = source.bar();
        bar.tab_lift_began(&mut source, 0, 7);
        let token = bar.out_token.clone();
        let before = source.identities();
        bar.out_finished(FINISHED);
        bar.drain_lifts(&mut source, FINISHED + 15_000);
        assert_eq!(
            bar.lifts.len(),
            1,
            "an unacknowledged lift is still valid at 15000 ms"
        );
        assert_eq!(source.identities(), before, "a missing ack closes nothing");
        if drain_first {
            bar.drain_lifts(&mut source, FINISHED + 15_001);
            assert!(
                bar.lifts.is_empty(),
                "the unacknowledged lift expires at 15001 ms"
            );
        }
        assert!(
            !bar.take(&mut source, &token, FINISHED + 15_001),
            "15001 ms after finish is late, with or without a prior drain"
        );
        bar.drain_lifts(&mut source, FINISHED + 15_001);
        assert!(bar.lifts.is_empty());
        assert_eq!(source.identities(), before);
    }
}

#[test]
fn sol_recent_tabs_remainder_t01_refused_receiver_never_acknowledges_or_closes_source() {
    let mut source = World::new(2);
    let mut bar = source.bar();
    bar.tab_lift_began(&mut source, 0, 7);
    let payload = bar.out_payload.clone();
    bar.out_finished(FINISHED);
    let mut destination = World::new(super::MAX);
    let mut receiver = destination.bar();
    receiver.self_pid = "222".into();
    assert!(!receiver.strip_drop(&mut destination, &payload, 0., 100_001));
    assert_eq!(destination.tabs.count(), super::MAX);
    // Any acknowledgement the receiver sends reaches the source, so the count below can go red.
    let taken = receiver.pump_taken();
    if let Some((_, token)) = &taken {
        bar.take(&mut source, token, FINISHED + 1);
    }
    bar.drain_lifts(&mut source, FINISHED + 15_001);
    assert_eq!(
        source.tabs.count(),
        2,
        "a refused drop closes no source tab"
    );
    assert_eq!(taken, None);
}
