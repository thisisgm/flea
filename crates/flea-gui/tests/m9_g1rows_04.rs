pub use flea_gui::{navigation, tabs};
#[path = "../src/tabs/test_support.rs"]
pub mod test_support;

use tabs::transfer::Rect;
use test_support::World;

#[test]
fn m9_g1rows_04_catcher_routes_body_strip_and_desktop() {
    let rect = Some(Rect {
        x: 1200.,
        y: -200.,
        width: 600.,
        height: 400.,
    });
    let strip = Some(Rect {
        x: 14.,
        y: 27.,
        width: 586.,
        height: 27.,
    });
    for (x, y, tear, reorder) in [
        (1300., -100., false, false),
        (1214., -160., false, true),
        (1900., -100., true, false),
    ] {
        let mut world = World::new(3);
        let mut bar = world.bar();
        bar.tab_lift_began(&mut world, 2, 1);
        let held = world.identities()[2];
        assert!(bar.tab_lift_moved(&mut world, 900., true));
        let result = bar.catcher_drop(&mut world, rect, strip, x, y, 2);
        assert_eq!(result.is_some(), tear);
        assert_eq!(world.tabs.count(), 3, "source stays until acknowledgment");
        assert_eq!(world.identities()[0] == held, reorder);
    }
}

#[test]
fn m9_g1rows_04_closed_tab_cannot_retarget_live_gesture() {
    let mut world = World::new(4);
    let mut bar = world.bar();
    bar.tab_lift_began(&mut world, 2, 1);
    bar.drag_moved(0., 4);
    world.tabs.close_at(&mut world.pane, 2);
    let remaining = world.identities();
    assert!(
        !bar.tab_lift_moved(&mut world, 900., true),
        "a live gesture whose identity closed never retargets or tears off"
    );
    assert!(bar.drag_from.is_none() && !bar.out_active);
    bar.tab_lift_ended(&mut world, 2);
    assert_eq!(
        world.identities(),
        remaining,
        "closing the held identity cancels its reorder"
    );
    assert!(bar.out_payload.is_empty());

    let mut world = World::new(4);
    let mut bar = world.bar();
    bar.tab_lift_began(&mut world, 2, 3);
    let held = world.identities()[2];
    bar.drag_moved(0., 4);
    world.tabs.close_at(&mut world.pane, 0);
    bar.tab_lift_ended(&mut world, 4);
    assert_eq!(
        world.identities()[0],
        held,
        "a preceding close preserves the held identity"
    );
}

#[test]
fn m9_g1rows_04_size_descending_transfer_overrides_receiver_name_ascending() {
    let mut source = World::new(2);
    source.pane.order = navigation::folder_state::Order {
        key: "size",
        desc: true,
    };
    let text = source
        .tabs
        .tab_payload(&source.pane, source.tabs.current(), "111", "sorted");
    let info = tabs::transfer::parse(&text).unwrap();
    assert_eq!(info.sort.as_ref().unwrap().order, source.pane.order);
    let mut receiver = World::new(1);
    assert_eq!(
        receiver.pane.order,
        navigation::folder_state::Order::default()
    );
    assert!(receiver
        .tabs
        .receive_tab(&mut receiver.pane, &text, None, "222"));
    assert_eq!(receiver.pane.order, source.pane.order);
    assert_eq!(
        receiver.pane.requests, 1,
        "one initial list, no post-landing resort"
    );
    let snap = &receiver.tabs.tabs.as_ref().unwrap().items[receiver.tabs.current()];
    assert_eq!(snap.order, source.pane.order);
}
