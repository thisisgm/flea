//! A peer's claim in flight: the peer's own lease expiry keeps it, Escape drops its lift, and a closed window frees it.
use super::carry::Carrying;
use super::carry_wire::{now, Offer, LIFETIME_MS};
use super::m9_g2rows_25_rig::{answer, edit, open, Handle};
use gpui::TestAppContext;
use std::time::Duration;

// Executor steps a click may take to start its claim before the test calls it stuck.
const CLAIM_START_STEPS: usize = 1_000;
// A peer lease short enough to lapse inside the test's clock advance.
const PEER_LEASE_MS: u128 = 1_000;
const PAST_THE_PEER_LEASE: Duration = Duration::from_secs(2);

// This process's own lift rewritten as another process's offer, as carry_accept leaves it on a peer.
fn peer_offer(
    cx: &mut TestAppContext,
) -> (
    Handle,
    Handle,
    Offer,
    impl FnMut() -> Vec<String>,
    impl FnMut() -> Vec<String>,
) {
    let (from, mut from_lines) = open(cx, "/source", &["a.txt"]);
    let (to, mut to_lines) = open(cx, "/dest", &["folder"]);
    from_lines();
    to_lines();
    edit(cx, from, |shell, window, cx| shell.carry_lift(window, cx));
    from_lines();
    answer(cx, from, &["/source/a.txt"]);
    let offer = cx.update(|cx| {
        let carrying = cx.global_mut::<Carrying>();
        let lifted = carrying.carry.riding().cloned().expect("the lift rides");
        let mut offer = Offer::new(lifted, (*carrying.flock).clone(), 0);
        offer.pid = std::process::id().wrapping_add(1);
        carrying.carry.cancel();
        carrying.ipc.owned = false;
        carrying.ipc.current = Some(offer.clone());
        offer
    });
    (from, to, offer, from_lines, to_lines)
}

fn claiming(cx: &mut TestAppContext) -> bool {
    cx.update(|cx| cx.global::<Carrying>().ipc.pending.is_some())
}

#[gpui::test]
fn carry_claim_window_closed_mid_claim_frees_the_peer_carry(cx: &mut TestAppContext) {
    let (from, to, _, _from_lines, _to_lines) = peer_offer(cx);
    edit(cx, to, |shell, window, cx| {
        assert!(shell.carry_land(0, "/dest/folder", true, false, window, cx))
    });
    let mut steps = 0;
    while !claiming(cx) {
        steps += 1;
        assert!(
            steps < CLAIM_START_STEPS && cx.executor().tick(),
            "THE_CLICK_MUST_START_A_CLAIM"
        );
    }
    edit(cx, to, |shell, _, _| shell.stop_for_app_quit());
    to.update(cx, |_, window, _| window.remove_window())
        .unwrap();
    cx.run_until_parked();
    assert!(!claiming(cx), "A_CLOSED_CLAIM_MUST_FREE_THE_PEER_CARRY");
    assert!(
        !edit(cx, from, |shell, window, cx| shell
            .carry_land(0, "/source", true, false, window, cx)),
        "ANOTHER_WINDOW_MUST_KEEP_ITS_CLICKS"
    );
    edit(cx, from, |shell, _, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn carry_claim_peer_expiry_keeps_the_offer_so_escape_drops_the_lift(cx: &mut TestAppContext) {
    let (from, to, mut offer, _from_lines, _to_lines) = peer_offer(cx);
    offer.expires = now() + PEER_LEASE_MS;
    cx.update(|cx| {
        let ipc = &mut cx.global_mut::<Carrying>().ipc;
        ipc.current = Some(offer.clone());
        ipc.pending = Some(offer.token.clone());
    });
    let expiring = offer.clone();
    edit(cx, to, |shell, window, cx| {
        shell.carry_expiry(expiring, window, cx)
    });
    cx.executor().advance_clock(PAST_THE_PEER_LEASE);
    cx.run_until_parked();
    let kept = cx.update(|cx| {
        cx.global::<Carrying>()
            .ipc
            .current
            .as_ref()
            .map(|current| current.token.clone())
    });
    assert_eq!(
        kept,
        Some(offer.token.clone()),
        "A_PEER_EXPIRY_MUST_KEEP_A_CLAIMED_OFFER"
    );
    assert!(edit(cx, to, |shell, window, cx| shell.carry_escape(window, cx)));
    let mut refresh = offer.clone();
    refresh.stamp += 1;
    refresh.token = format!("{}-refresh", offer.token);
    refresh.expires = now() + LIFETIME_MS;
    let accepted = cx.update(|cx| cx.global_mut::<Carrying>().ipc.accept(&refresh));
    assert!(!accepted, "ESCAPE_MID_CLAIM_MUST_DROP_THE_LIFT");
    edit(cx, from, |shell, _, _| shell.stop_for_app_quit());
    edit(cx, to, |shell, _, _| shell.stop_for_app_quit());
}
