use super::{
    test_support::World,
    transfer::{self, PEEK_FIRST},
};

#[test]
fn u25_model_gesture_without_a_lift_never_offers_the_previous_lifts_payload() {
    let mut world = World::new(2);
    let mut bar = world.bar();
    bar.tab_lift_began(&mut world, 0, 1);
    bar.tab_lift_ended(&mut world, 2);
    assert!(
        bar.out_payload.is_empty(),
        "an internal release retires its offer"
    );
    assert!(!bar.tab_lift_moved(&mut world, 900., true));
    assert!(!bar.out_active);
}

#[test]
fn u25_model_second_receive_before_landing_keeps_the_first_tab_unacknowledged() {
    let mut world = World::new(1);
    let mut bar = world.bar();
    for (token, path) in [("first", "/first"), ("second", "/second")] {
        let payload = transfer::payload("222", token, path, "list", "");
        let info = transfer::parse(&payload).unwrap();
        assert!(bar.accept_tab_drop(&mut world, &payload, &info, None, 1));
        bar.on_peeked(&mut world, 0, path, false, false, false, PEEK_FIRST);
        assert!(
            world.pane.busy,
            "both receives precede their destination landing"
        );
    }
    assert_eq!(world.tabs.count(), 3);
    assert_eq!(bar.pump_taken(), Some(("222".into(), "first".into())));
    assert_eq!(bar.taken_exited(), Some(("222".into(), "second".into())));
    assert_eq!(bar.taken_exited(), None);
}

#[test]
fn u25_model_an_accepted_receive_is_acknowledged_before_its_listing_settles() {
    let mut source = World::new(2);
    let mut lifting = source.bar();
    lifting.tab_lift_began(&mut source, 0, 1);
    let payload = lifting.out_payload.clone();
    let info = transfer::parse(&payload).unwrap();
    lifting.out_finished(2);
    let mut receiver = World::new(1);
    let mut bar = receiver.bar();
    bar.self_pid = "222".into();
    assert!(bar.accept_tab_drop(&mut receiver, &payload, &info, None, 3));
    bar.on_peeked(
        &mut receiver,
        0,
        &info.path,
        false,
        false,
        false,
        PEEK_FIRST,
    );
    receiver.pane.busy = false;
    let (pid, token) = bar
        .pump_taken()
        .expect("the receipt does not wait for the destination listing");
    assert_eq!(pid, "111");
    assert!(lifting.take(&mut source, &token, 4));
    assert_eq!(source.tabs.count(), 1);
    assert_eq!(receiver.tabs.count(), 2);
    assert!(!lifting.take(&mut source, &token, 5));
    assert_eq!(bar.taken_exited(), None);
}

#[test]
fn u25_finished_native_drag_retires_its_offer_while_the_lift_waits() {
    let mut world = World::new(2);
    let mut bar = world.bar();
    bar.tab_lift_began(&mut world, 0, 1);
    assert!(bar.tab_lift_moved(&mut world, 900., true));
    bar.out_finished(2);
    assert!(bar.out_payload.is_empty());
    assert_eq!(bar.lifts.len(), 1);
    assert!(!bar.tab_lift_moved(&mut world, 900., true));
}

#[test]
fn u25_live_gesture_refuses_a_retired_own_token() {
    let mut world = World::new(3);
    let mut bar = world.bar();
    bar.tab_lift_began(&mut world, 0, 1);
    let retired = bar.out_payload.clone();
    bar.tab_lift_ended(&mut world, 2);
    bar.tab_lift_began(&mut world, 2, 3);
    assert!(bar.tab_lift_moved(&mut world, 900., true));
    let before = world.identities();
    assert!(!bar.strip_drop(&mut world, &retired, 0., 4));
    assert_eq!(world.identities(), before);
    assert!(!bar.own_accepted);
}

#[test]
fn u25_overlapping_folder_checks_refuse_the_second_offer() {
    let mut world = World::new(1);
    let mut bar = world.bar();
    let first = transfer::payload("222", "first", "/first", "list", "");
    let second = transfer::payload("333", "second", "/second", "list", "");
    assert!(bar.strip_drop(&mut world, &first, 0., 1));
    assert!(!bar.strip_drop(&mut world, &second, 0., 2));
    assert_eq!(bar.pending.as_ref().unwrap().token, "first");
    bar.on_peeked(&mut world, 0, "/first", false, false, false, PEEK_FIRST);
    assert_eq!(bar.pump_taken(), Some(("222".into(), "first".into())));
    assert_eq!(bar.taken_exited(), None);
}

#[test]
fn u25_timeout_keeps_the_source_and_refuses_its_late_acknowledgment() {
    let mut world = World::new(2);
    let mut bar = world.bar();
    bar.tab_lift_began(&mut world, 0, 1);
    let token = bar.out_token.clone();
    bar.out_finished(2);
    bar.drain_lifts(&mut world, 3 + transfer::ACK_WAIT_MS);
    assert_eq!(world.tabs.count(), 2);
    assert!(!bar.take(&mut world, &token, 4 + transfer::ACK_WAIT_MS));
    assert!(bar.lifts.is_empty());
    assert!(bar.out_payload.is_empty());
}

#[test]
fn u25_failed_folder_check_keeps_the_source_and_adds_no_receiver() {
    let mut world = World::new(1);
    let mut bar = world.bar();
    let payload = transfer::payload("222", "failed", "/gone", "list", "");
    assert!(bar.strip_drop(&mut world, &payload, 0., 1));
    bar.on_peeked(&mut world, 0, "/gone", false, true, false, PEEK_FIRST);
    assert_eq!(world.tabs.count(), 1);
    assert_eq!(bar.pump_taken(), None);
    assert!(bar.pending.is_none());
}

#[test]
fn u25_unrelated_peek_leaves_the_tab_check_pending() {
    let mut world = World::new(1);
    let mut bar = world.bar();
    let payload = transfer::payload("222", "pending", "/files/child", "list", "");
    assert!(bar.strip_drop(&mut world, &payload, 0., 1));
    bar.on_peeked(&mut world, 0, "/files/child", false, false, false, 320);
    assert_eq!(world.tabs.count(), 1);
    assert_eq!(bar.pending.as_ref().unwrap().token, "pending");
    assert_eq!(bar.pump_taken(), None);
}

// Tabs.js tearOff refuses only a listing in flight or a rename; a live search still tears off with the tab's sort.
#[test]
fn u25_a_current_tab_under_a_live_search_tears_off_with_its_sort() {
    let mut world = World::new(2);
    world.pane.search = true;
    let mut bar = world.bar();
    bar.tab_lift_began(&mut world, 1, 1);
    let sort = transfer::parse(&bar.out_payload)
        .and_then(|info| info.sort)
        .expect("the payload carries a sort");
    assert_eq!(sort.path, "/files/1");
    assert_eq!(sort.order, world.pane.order);
    assert!(bar.tab_lift_moved(&mut world, 900., true));
    assert!(world.pane.said.is_empty(), "{:?}", world.pane.said);
}

// A cancelled lift keeps no rectangle, even when its query answers with a matching window.
#[test]
fn u25_a_cancelled_geometry_query_answers_nothing() {
    let mut geometry = transfer::Geometry::default();
    let strip = transfer::Rect {
        x: 0.,
        y: 0.,
        width: 600.,
        height: 40.,
    };
    assert!(geometry.begin("1-1", strip));
    geometry.cancel();
    let clients = r#"[{"pid":111,"at":[0,0],"size":[900,500],"mapped":true,"hidden":false}]"#;
    assert!(!geometry.finished(clients, 111));
    assert_eq!(geometry.rect, None);
    assert_eq!(geometry.asked, None);
}
