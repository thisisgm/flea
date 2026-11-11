//! Offer leases do not impose a lifetime on the source's own carry.
use super::{answer, edit, open, Endpoint, Fixture};
use std::time::Duration;

#[test]
fn carry_lifetime_single_window_still_lands_after_sixty_seconds() {
    let fixture = Fixture::new();
    let mut cx = crate::list::bench::real_cx();
    let (window, mut lines) = open(
        &mut cx,
        fixture.root.join("source").to_str().unwrap(),
        &["a.txt"],
    );
    lines();
    edit(&mut cx, window, |shell, window, cx| {
        shell.carry_lift(window, cx)
    });
    answer(
        &mut cx,
        window,
        &[fixture.root.join("source/a.txt").to_str().unwrap()],
    );
    lines();
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(61));
    cx.run_until_parked();
    assert!(
        edit(&mut cx, window, |shell, _, cx| shell
            .carry_riding_or_pending(cx)),
        "SOURCE_CARRY_MUST_NOT_EXPIRE"
    );
    edit(&mut cx, window, |shell, window, cx| {
        shell.carry_land(
            0,
            fixture.root.join("dest/folder").to_str().unwrap(),
            false,
            false,
            window,
            cx,
        )
    });
    cx.run_until_parked();
    assert!(
        lines()
            .iter()
            .any(|line| line.contains(r#""c":"collisions""#)),
        "AGED_LOCAL_CARRY_MUST_REACH_OP1"
    );
}

#[test]
fn carry_lifetime_peer_drops_a_lapsed_offer_without_cancelling_source() {
    let fixture = Fixture::new();
    let source = Endpoint::new(&fixture.root, "source");
    let peer = Endpoint::new(&fixture.root, "dest");
    source.offer_to(&peer);
    peer.call("fixtureAdvance", vec!["61".into()]);
    assert_eq!(
        peer.call("fixtureActive", vec![]),
        "false",
        "PEER_MUST_DROP_LAPSED_OFFER"
    );
    assert_eq!(
        source.call("fixtureActive", vec![]),
        "true",
        "PEER_EXPIRY_MUST_NOT_CANCEL_SOURCE"
    );
    assert!(source
        .call("carryPeek", vec![String::new()])
        .starts_with('{'));
}

#[test]
fn carry_lifetime_refresh_keeps_a_cross_process_landing_after_ninety_seconds() {
    let fixture = Fixture::new();
    let source = Endpoint::new(&fixture.root, "source");
    let first = source.call("carryPeek", vec![String::new()]);
    for _ in 0..3 {
        source.call("fixtureAdvance", vec!["30".into()]);
    }
    assert_eq!(
        source.call("fixtureActive", vec![]),
        "true",
        "REFRESH_MUST_KEEP_SOURCE_CARRY"
    );
    let refreshed = source.call("carryPeek", vec![String::new()]);
    let origin = |text: &str| {
        flea::jsondoc::parse(text)
            .unwrap()
            .get("liftStamp")
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned()
    };
    assert_eq!(
        origin(&first),
        origin(&refreshed),
        "REFRESH_MUST_KEEP_LIFT_ORDER"
    );
    let token = |text: &str| {
        flea::jsondoc::parse(text)
            .unwrap()
            .get("token")
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned()
    };
    assert_ne!(
        token(&first),
        token(&refreshed),
        "SOURCE_MUST_REFRESH_OFFER"
    );
    let peer = Endpoint::new(&fixture.root, "dest");
    peer.call("fixtureAdvance", vec!["90".into()]);
    source.offer_to(&peer);
    let sent = peer.call("fixtureLand", vec![]);
    assert!(
        sent.contains(r#""c":"collisions""#) && sent.contains(r#""op":"move""#),
        "REFRESHED_CARRY_MUST_REACH_OP1: {sent}"
    );
    assert_eq!(source.call("fixtureActive", vec![]), "false");
}

#[test]
fn carry_lifetime_refresh_keeps_previous_lease_escape_and_claim_valid() {
    for function in ["carryCancel", "carryClaim"] {
        let fixture = Fixture::new();
        let source = Endpoint::new(&fixture.root, "source");
        let first = source.call("carryPeek", vec![String::new()]);
        let first = flea::jsondoc::parse(&first).unwrap();
        let old_token = first.get("token").unwrap().as_str().unwrap().to_owned();
        source.call("fixtureAdvance", vec!["30".into()]);
        assert_eq!(source.call(function, vec![old_token]), "true");
        assert_eq!(
            source.call("fixtureActive", vec![]),
            "false",
            "REFRESH_MUST_NOT_INVALIDATE_PEER_ESCAPE_OR_CLAIM"
        );
    }
}
