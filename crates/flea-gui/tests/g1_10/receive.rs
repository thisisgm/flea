use super::*;
use flea_gui::tabs::{
    state::Open,
    transfer::{parse, payload, ACK_WAIT_MS, PEEK_FIRST},
};

const REFUSED: &str = "That tab could not be received.";

fn panes(paths: &[&str]) -> World {
    World {
        sides: paths
            .iter()
            .map(|path| Some(Side::new(Fake::at(path))))
            .collect(),
    }
}

fn offer(world: &mut World, bar: &mut Bar, token: &str, path: &str) {
    let text = payload("222", token, path, "list", "");
    bar.accept_tab_drop(world, &text, &parse(&text).unwrap(), None, NOW);
}

fn answer(world: &mut World, bar: &mut Bar, side: usize, path: &str, failed: bool) {
    bar.on_peeked(world, side, path, false, failed, false, PEEK_FIRST);
}

fn total(world: &mut World) -> usize {
    (0..world.sides.len())
        .map(|side| world.side(side).count())
        .sum()
}

#[test]
fn a_pane_switch_refuses_the_pending_drop_and_no_late_answer_revives_it() {
    let mut world = panes(&["/tmp/first", "/tmp/second"]);
    let mut bar = bar();
    offer(&mut world, &mut bar, "old", "/tmp/old");
    assert_eq!(
        bar.pending.as_ref().map(|pending| pending.side),
        Some(0),
        "drop captures its receiving pane"
    );
    assert_eq!(
        world.side(0).pane.peeks,
        [("/tmp/old".to_string(), 2, false, false)],
        "request carries reserved peek discriminator and flags"
    );
    bar.set_pane(&mut world, Some(1));
    assert!(bar.pending.is_none(), "pane switch clears pending drop");
    assert_eq!(
        world.side(1).pane.said,
        [REFUSED],
        "pane switch visibly refuses on active pane"
    );
    assert!(
        acks(&mut bar).is_empty(),
        "pane switch sends no acknowledgment"
    );
    assert_eq!(bar.deadline, None, "pane switch stops deadline");
    answer(&mut world, &mut bar, 0, "/tmp/old", false);
    assert_eq!(
        total(&mut world),
        2,
        "original backend late reply opens no tab"
    );
    assert!(
        acks(&mut bar).is_empty(),
        "original backend late reply sends no acknowledgment"
    );
    assert_eq!(
        world.side(1).pane.said,
        [REFUSED],
        "original backend late reply adds no refusal"
    );
    answer(&mut world, &mut bar, 1, "/tmp/old", false);
    assert_eq!(
        total(&mut world),
        2,
        "active backend late reply opens no tab"
    );
    assert!(
        acks(&mut bar).is_empty(),
        "active backend late reply sends no acknowledgment"
    );
    assert_eq!(
        world.side(1).pane.said,
        [REFUSED],
        "active backend late reply adds no refusal"
    );
    offer(&mut world, &mut bar, "new", "/tmp/new");
    assert_eq!(
        world.side(1).pane.peeks.len(),
        1,
        "next pane can request another drop"
    );
    answer(&mut world, &mut bar, 1, "/tmp/new", false);
    assert_eq!(
        world.side(1).pane.path,
        "/tmp/new",
        "new pane receives next drop"
    );
    assert_eq!(
        acks(&mut bar),
        ["new"],
        "only received drop is acknowledged"
    );
    assert_eq!(bar.deadline, None, "received drop stops deadline");
}

#[test]
fn a_new_drop_for_the_same_path_never_takes_the_previous_panes_late_answer() {
    let mut world = panes(&["/tmp/old-pane", "/tmp/active-pane"]);
    let mut bar = bar();
    offer(&mut world, &mut bar, "previous", "/tmp/old");
    bar.set_pane(&mut world, Some(1));
    offer(&mut world, &mut bar, "current", "/tmp/old");
    answer(&mut world, &mut bar, 0, "/tmp/old", false);
    assert_eq!(
        total(&mut world),
        2,
        "original backend cannot receive same-path pending drop"
    );
    assert!(
        acks(&mut bar).is_empty(),
        "original backend cannot acknowledge same-path pending drop"
    );
    assert_eq!(
        world.side(1).pane.said,
        [REFUSED],
        "original backend adds no same-path refusal"
    );
    assert_eq!(
        bar.pending.as_ref().map(|pending| pending.token.as_str()),
        Some("current"),
        "original backend leaves active drop pending"
    );
    answer(&mut world, &mut bar, 1, "/tmp/old", false);
    assert_eq!(
        world.side(1).pane.path,
        "/tmp/old",
        "active backend receives same-path pending drop"
    );
    assert_eq!(
        acks(&mut bar),
        ["current"],
        "active backend acknowledges same-path pending drop"
    );
}

#[test]
fn a_lost_answer_is_refused_at_the_deadline_and_the_next_drop_still_works() {
    let mut world = panes(&["/tmp/lost"]);
    let mut bar = bar();
    offer(&mut world, &mut bar, "lost", "/tmp/missing-reply");
    assert_eq!(
        bar.deadline,
        Some(NOW + ACK_WAIT_MS),
        "deadline uses existing acknowledgment bound"
    );
    assert!(
        !bar.expire(&mut world, NOW + ACK_WAIT_MS - 1),
        "the deadline has not passed a moment early"
    );
    assert!(
        bar.expire(&mut world, NOW + ACK_WAIT_MS),
        "receiver has a deadline that fires"
    );
    assert!(
        bar.pending.is_none(),
        "lost reply clears pending drop at deadline"
    );
    assert_eq!(
        world.side(0).pane.said,
        [REFUSED],
        "deadline visibly refuses"
    );
    assert!(
        acks(&mut bar).is_empty(),
        "deadline sends no acknowledgment"
    );
    answer(&mut world, &mut bar, 0, "/tmp/missing-reply", false);
    assert_eq!(
        world.side(0).count(),
        1,
        "late reply cannot receive expired drop"
    );
    assert!(
        acks(&mut bar).is_empty(),
        "late reply sends no acknowledgment"
    );
    offer(&mut world, &mut bar, "retry", "/tmp/retry");
    assert_eq!(
        world.side(0).pane.peeks.len(),
        2,
        "deadline permits next drop"
    );
    bar.on_peeked(&mut world, 0, "/tmp/retry", true, false, false, PEEK_FIRST);
    assert_eq!(
        bar.pending.as_ref().map(|pending| pending.token.as_str()),
        Some("retry"),
        "unrelated reply leaves pending drop"
    );
    assert!(
        bar.deadline.is_some(),
        "unrelated reply leaves deadline running"
    );
    bar.on_peeked(
        &mut world,
        0,
        "/tmp/retry",
        false,
        false,
        false,
        PEEK_FIRST + 1,
    );
    bar.on_peeked(&mut world, 0, "/tmp/retry", false, false, true, PEEK_FIRST);
    assert!(
        bar.pending.is_some(),
        "and neither does a peek with another row count or a hidden-last answer"
    );
    answer(&mut world, &mut bar, 0, "/tmp/retry", false);
    assert_eq!(
        world.side(0).pane.path,
        "/tmp/retry",
        "retry receives its own folder"
    );
    assert_eq!(acks(&mut bar), ["retry"], "retry acknowledges once");
    assert_eq!(bar.deadline, None, "retry receipt stops deadline");
    assert!(
        !bar.expire(&mut world, NOW + 10 * ACK_WAIT_MS),
        "a stopped deadline fires nothing"
    );
    assert_eq!(
        world.side(0).pane.said.len(),
        1,
        "stopped deadline reports no second refusal"
    );
}

#[test]
fn a_removed_pane_or_a_failed_check_refuses_and_sends_nothing() {
    let mut world = panes(&["/tmp/gone"]);
    let mut bar = bar();
    offer(&mut world, &mut bar, "gone", "/tmp/gone-drop");
    bar.set_pane(&mut world, None);
    assert!(bar.pending.is_none(), "removed pane clears pending drop");
    assert_eq!(
        world.side(0).pane.said,
        [REFUSED],
        "removed pane refuses on captured pane"
    );
    assert!(
        acks(&mut bar).is_empty(),
        "removed pane sends no acknowledgment"
    );

    let mut world = panes(&["/tmp/control"]);
    let mut bar = self::bar();
    offer(&mut world, &mut bar, "failed", "/tmp/not-there");
    answer(&mut world, &mut bar, 0, "/tmp/not-there", true);
    assert!(bar.pending.is_none(), "failed peek clears pending drop");
    assert_eq!(
        world.side(0).pane.said,
        ["That folder is no longer there."],
        "failed peek keeps existing refusal"
    );
    assert!(
        acks(&mut bar).is_empty(),
        "failed peek sends no acknowledgment"
    );
    assert_eq!(bar.deadline, None, "failed peek stops deadline");

    let mut world = panes(&["/tmp/full"]);
    {
        let side = world.side(0);
        for _ in 0..8 {
            side.act("tabNew");
        }
    }
    let mut bar = self::bar();
    offer(&mut world, &mut bar, "full", "/tmp/elsewhere");
    assert!(
        bar.pending.is_none() && world.side(0).pane.peeks.is_empty(),
        "a full strip takes no drop and sends no check"
    );
}

fn listing_starts(pane: &mut Fake, path: &str, _: Open) {
    pane.busy = true;
    pane.listed.push(path.into());
    pane.list_requests += 1;
}

#[test]
fn an_acknowledgment_leaves_once_the_receiving_listing_starts_and_closes_the_source_once() {
    let mut receiver = panes(&["/tmp/before-list"]);
    receiver.side(0).pane.on_open = Some(listing_starts);
    let mut receiving = bar();
    let mut source = World::one(Side::pair("/tmp/awaiting-list"));
    let mut lifting = bar();
    lifting.tab_lift_began(&mut source, 1, NOW);
    let token = lifting.out_token.clone();
    lifting.out_finished(NOW);
    offer(&mut receiver, &mut receiving, &token, "/tmp/awaiting-list");
    assert_eq!(
        source.side(0).count(),
        2,
        "unacknowledged drop keeps both source tabs"
    );
    answer(
        &mut receiver,
        &mut receiving,
        0,
        "/tmp/awaiting-list",
        false,
    );
    assert!(
        receiver.side(0).pane.busy,
        "successful peek starts the receiving list"
    );
    let sent = acks(&mut receiving);
    assert_eq!(
        sent.len(),
        1,
        "a successful peek acknowledges the drop once"
    );
    for token in &sent {
        lifting.take(&mut source, token, NOW + 50);
    }
    assert_eq!(
        source.side(0).count(),
        1,
        "the acknowledged tab leaves the source strip"
    );
}

#[test]
fn acknowledgments_queue_one_at_a_time_behind_the_call_out() {
    let mut bar = bar();
    bar.send_taken("222", "a");
    bar.send_taken("333", "b");
    assert_eq!(
        bar.pump_taken(),
        Some(("222".into(), "a".into())),
        "the first ack runs at once"
    );
    bar.send_taken("444", "c");
    assert_eq!(
        bar.pump_taken(),
        None,
        "a second waits behind the call out rather than vanishing"
    );
    assert_eq!(
        bar.taken_exited(),
        Some(("333".into(), "b".into())),
        "each exit starts the next, in order"
    );
    assert_eq!(
        bar.taken_exited(),
        Some(("444".into(), "c".into())),
        "including one queued while another ran"
    );
    assert_eq!(bar.taken_exited(), None, "and the queue ends empty");
}
