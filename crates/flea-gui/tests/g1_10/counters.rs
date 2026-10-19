use super::*;
use flea::jsondoc;
use flea_gui::tabs::{
    bar::{tab_width, Hit, MenuAsk, HEIGHT, HIT_MIN, HOVER_SWITCH_MS, MENU_GAP, PAD_X},
    state::{Plan, Remember, Tab, MAX},
};

fn spread(paths: &[&str]) -> Side {
    let mut side = Side::new(Fake::at(paths[0]));
    for path in &paths[1..] {
        side.state.open_new(&mut side.pane, Some((*path).into()));
    }
    side
}

fn plain<T: Send + Sync + 'static>() {}

#[test]
fn a_reorder_by_key_or_drag_lists_nothing_and_asks_the_backend_for_nothing() {
    let mut world = World::one(spread(&["/tmp/a", "/tmp/b", "/tmp/c"]));
    let before = (
        world.side(0).pane.list_requests,
        world.side(0).pane.listed.clone(),
    );
    {
        let side = world.side(0);
        assert!(side.act("tabMoveLeft"));
        assert!(side.act("tabMoveLeft"));
        assert!(
            side.act("tabMoveLeft"),
            "a move at the left end clamps rather than wrapping"
        );
        assert!(side.act("tabMoveRight"));
        side.state.move_tab(&mut side.pane, 0, 2);
    }
    let mut bar = bar();
    bar.drag_started(Some(0));
    bar.drag_moved(f64::from(bar.tab_width(3)) * 3., 3);
    bar.drag_finished(&mut world, NOW);
    let side = world.side(0);
    assert_eq!(
        (side.pane.list_requests, side.pane.listed.clone()),
        before,
        "no reorder re-lists a folder"
    );
    assert!(
        side.pane.sorted.is_empty() && side.pane.windows.is_empty() && side.pane.peeks.is_empty(),
        "and none asks for a sort, a window or a peek"
    );
    assert_eq!(
        side.paths(),
        ["/tmp/b", "/tmp/a", "/tmp/c"],
        "the strip still moved"
    );
    assert_eq!(
        side.state.path_at(side.state.current(), &side.pane.path),
        "/tmp/c",
        "with the current tab still current"
    );
}

#[test]
fn a_switch_makes_one_listing_carrying_the_tabs_own_order_and_spends_it_once() {
    let mut side = Side::new(Fake::at("/tmp/a"));
    side.pane.order = order("size", true);
    side.state.open_new(&mut side.pane, Some("/tmp/b".into()));
    side.pane.order = order("name", false);
    let listings = side.pane.list_requests;
    side.state.select_at(&mut side.pane, 0);
    assert_eq!(
        side.pane.list_requests - listings,
        1,
        "one listing per switch"
    );
    assert_eq!(
        side.pane.opened.last().and_then(|open| open.order),
        Some(order("size", true)),
        "the listing asks for the tab's own order"
    );
    side.state.apply_pending(&mut side.pane);
    side.state.apply_pending(&mut side.pane);
    assert_eq!(
        side.pane.sorted,
        ["size:true"],
        "the first reply in another order sorts once, and the order is spent"
    );
    assert_eq!(
        side.pane.windows,
        ["0:40"],
        "with the one window that sort selects"
    );
    assert_eq!(
        side.pane.list_requests - listings,
        1,
        "and no second listing follows"
    );
}

#[test]
fn a_hidden_tab_is_plain_data_that_never_lists_or_holds_a_backend() {
    plain::<Tab>();
    plain::<TabState>();
    let mut side = Side::new(Fake::at("/tmp/t1"));
    for n in 2..=MAX {
        side.state
            .open_new(&mut side.pane, Some(format!("/tmp/t{n}")));
    }
    assert_eq!(side.count(), MAX);
    assert_eq!(
        side.pane.list_requests,
        MAX - 1,
        "each open lists its own folder once, and the tabs left behind list nothing"
    );
    let listings = side.pane.list_requests;
    for index in [0, 4, 8, 4] {
        side.state.select_at(&mut side.pane, index);
    }
    assert_eq!(
        side.pane.list_requests - listings,
        4,
        "a switch lists only the tab it shows"
    );
    let frame = bar().frame(&side.state, &side.pane.path, "/home/gm", "Home");
    assert_eq!(
        (frame.labels.len(), frame.slots.len()),
        (MAX, MAX),
        "nine tabs are nine labels in one strip, not nine views"
    );
}

#[test]
fn the_remembered_strip_writes_once_per_change_and_never_per_frame() {
    let mut remember = Remember::seeded(&jsondoc::parse("{}").unwrap());
    let plan = Plan {
        paths: vec!["/tmp/a".into(), "/tmp/b".into()],
        index: 1,
    };
    assert!(remember.owed("/tmp/b", &plan, false, false).is_some());
    for _ in 0..600 {
        assert_eq!(
            remember.owed("/tmp/b", &plan, false, false),
            None,
            "an unchanged strip owes nothing, however often it paints"
        );
    }
    assert_eq!(remember.writes, 1);
    assert_eq!(
        remember.owed("/tmp/c", &plan, false, true),
        None,
        "a listing out writes nothing"
    );
    assert_eq!(
        remember.owed("/tmp/c", &plan, true, false),
        None,
        "a Dual window writes nothing"
    );
    assert_eq!(
        remember.owed("", &plan, false, false),
        None,
        "no folder yet writes nothing"
    );
    assert_eq!(remember.writes, 1);
}

#[test]
fn the_strip_shares_its_width_and_keeps_the_close_slot_whole() {
    assert_eq!(
        tab_width(500., 3, 12.),
        159.,
        "three tabs share the strip past its close slot"
    );
    assert_eq!(
        tab_width(2000., 2, 12.),
        196.,
        "a wide strip stops at a caption's twelve cells"
    );
    assert_eq!(
        tab_width(100., 9, 12.),
        HIT_MIN * 3.,
        "and never shrinks under three close slots"
    );
    let bar = bar();
    let width = bar.tab_width(3);
    assert_eq!(
        bar.hit(PAD_X - 1., 3),
        Hit::Nothing,
        "the pad before the first tab names nothing"
    );
    assert_eq!(bar.hit(PAD_X + 1., 3), Hit::Tab(0));
    assert_eq!(
        bar.hit(PAD_X + width - HIT_MIN, 3),
        Hit::Close(0),
        "the close slot starts a full slot before the tab's edge"
    );
    assert_eq!(bar.hit(PAD_X + width + 1., 3), Hit::Tab(1));
    assert_eq!(
        bar.hit(PAD_X + 3. * width + 1., 3),
        Hit::New,
        "New tab sits one slot past the last"
    );
    assert_eq!(bar.hit(PAD_X + 3. * width + HIT_MIN, 3), Hit::Nothing);
}

#[test]
fn only_the_close_mark_under_the_pointer_brightens_and_a_drag_names_none() {
    let side = spread(&["/tmp/a", "/tmp/b", "/tmp/c"]);
    let mut bar = bar();
    let width = bar.tab_width(3);
    let mark = PAD_X + 2. * width - HIT_MIN / 2.;
    assert!(bar.hovered(Some(mark), 3), "entering a mark is a change");
    assert_eq!(
        bar.close_hover,
        Some(1),
        "the second tab's mark is under it"
    );
    assert!(
        !bar.hovered(Some(mark + 1.), 3),
        "moving inside it is no change"
    );
    let frame = bar.frame(&side.state, "/tmp/c", "/home/gm", "Home");
    assert_eq!(frame.close_hover, Some(1), "and the frame draws it bright");
    assert!(
        bar.hovered(Some(PAD_X + width + 1.), 3),
        "the tab body is not the mark"
    );
    assert_eq!(bar.close_hover, None);
    bar.hovered(Some(mark), 3);
    bar.settle(Some(3));
    assert_eq!(bar.close_hover, Some(1), "an unchanged strip keeps it");
    bar.settle(Some(2));
    assert_eq!(bar.close_hover, None, "a close hands it to no other tab");
    bar.hovered(Some(mark), 3);
    bar.settle(None);
    assert_eq!(bar.close_hover, None, "and a hidden strip keeps none");
    bar.drag_started(Some(0));
    assert!(
        !bar.hovered(Some(mark), 3),
        "a running drag brightens no mark"
    );
    assert_eq!(bar.close_hover, None);
}

#[test]
fn a_secondary_press_asks_the_tab_menu_three_under_the_strip_at_the_pointer() {
    let bar = bar();
    let width = bar.tab_width(3);
    assert_eq!(
        bar.menu_at(PAD_X + width + 40., 3),
        Some(MenuAsk {
            index: 1,
            x: PAD_X + width + 40.,
            y: HEIGHT + MENU_GAP,
        }),
        "the menu names the tab pressed and opens 3 under the 27 strip"
    );
    assert_eq!(
        HEIGHT + MENU_GAP,
        30.,
        "the board's 57 top, 3 under the strip's 54 bottom"
    );
    assert_eq!(
        bar.menu_at(PAD_X + 3. * width - 2., 3).map(|ask| ask.index),
        Some(2),
        "a press on a tab's mark asks for that tab"
    );
    assert_eq!(
        bar.menu_at(PAD_X + 3. * width + 1., 3),
        None,
        "New tab has no tab menu"
    );
    assert_eq!(bar.menu_at(PAD_X - 1., 3), None, "nor has the pad");
}

#[test]
fn a_press_selects_closes_or_opens_and_a_middle_press_closes_whatever_it_lands_on() {
    let mut world = World::one(spread(&["/tmp/a", "/tmp/b", "/tmp/c"]));
    let mut bar = bar();
    let width = bar.tab_width(3);
    bar.tap(&mut world, PAD_X + 1., false);
    assert_eq!(world.side(0).state.current(), 0, "a left press selects");
    bar.tap(&mut world, PAD_X + width + 1., true);
    assert_eq!(
        world.side(0).paths(),
        ["/tmp/a", "/tmp/c"],
        "a middle press on a tab's body closes it"
    );
    let width = bar.tab_width(2);
    bar.tap(&mut world, PAD_X + 2. * width - 1., false);
    assert_eq!(
        world.side(0).paths(),
        ["/tmp/a"],
        "a left press on the mark closes"
    );
    bar.tap(&mut world, PAD_X + 1., true);
    assert_eq!(
        world.side(0).pane.last_said(),
        "The last tab cannot be closed",
        "and the last tab refuses either way"
    );
    let width = bar.tab_width(1);
    bar.tap(&mut world, PAD_X + width + 1., true);
    assert_eq!(
        world.side(0).count(),
        1,
        "a middle press on New opens nothing"
    );
    bar.tap(&mut world, PAD_X + width + 1., false);
    assert_eq!(world.side(0).count(), 2, "a left press on New opens a tab");
    bar.pane = None;
    bar.tap(&mut world, PAD_X + 1., false);
    assert_eq!(
        world.side(0).count(),
        2,
        "a strip standing for no pane does nothing"
    );
}

#[test]
fn a_resting_drag_switches_once_after_the_delay_and_a_refused_enter_arms_nothing() {
    let mut world = World::one(spread(&["/tmp/a", "/tmp/b"]));
    let mut bar = bar();
    bar.hover_entered(0, false, NOW);
    assert!(bar.hover.is_none(), "a refused enter starts no timer");
    bar.hover_entered(0, true, NOW);
    assert!(
        !bar.hover_tick(&mut world, NOW + HOVER_SWITCH_MS - 1),
        "the drag crossing a tab does not switch"
    );
    assert!(
        bar.hover_tick(&mut world, NOW + HOVER_SWITCH_MS),
        "a drag resting on it does"
    );
    assert_eq!(world.side(0).state.current(), 0);
    assert!(
        !bar.hover_tick(&mut world, NOW + 2 * HOVER_SWITCH_MS),
        "once"
    );
    bar.hover_entered(0, true, NOW);
    assert!(
        !bar.hover_tick(&mut world, NOW + HOVER_SWITCH_MS),
        "resting on the current tab switches nothing"
    );
    bar.hover_entered(1, true, NOW);
    bar.hover_exited();
    assert!(
        !bar.hover_tick(&mut world, NOW + HOVER_SWITCH_MS),
        "leaving before the delay cancels it"
    );
    assert_eq!(world.side(0).state.current(), 0);
}

#[test]
fn a_frame_draws_the_held_tab_ghosted_in_its_slot_with_the_line_and_the_focus_stop() {
    let side = spread(&["/tmp/a", "/tmp/b", "/tmp/c"]);
    let mut bar = bar();
    let still = bar.frame(&side.state, &side.pane.path, "/home/gm", "Home");
    assert_eq!(
        (still.slots.clone(), still.ghost, still.line, still.focus),
        (vec![0, 1, 2], None, None, None)
    );
    assert_eq!(still.current, 2);
    bar.drag_started(Some(0));
    bar.drag_moved(f64::from(bar.tab_width(3)) * 2. + 1., 3);
    let held = bar.frame(&side.state, &side.pane.path, "/home/gm", "Home");
    assert_eq!(
        held.slots,
        [1, 0, 2],
        "the held tab draws in the slot it would land in"
    );
    assert_eq!((held.ghost, held.line), (Some(0), Some(1)));
    assert_eq!(held.labels, still.labels, "a drag relabels nothing");
    bar.drag_cancelled();
    assert_eq!(
        bar.frame(&side.state, &side.pane.path, "/home/gm", "Home")
            .slots,
        [0, 1, 2],
        "a cancelled drag puts every tab back"
    );
    bar.stop_focused = true;
    bar.focused_index = 3;
    assert_eq!(
        bar.frame(&side.state, &side.pane.path, "/home/gm", "Home")
            .focus,
        Some(3),
        "the stop past the last tab is New tab"
    );
}
