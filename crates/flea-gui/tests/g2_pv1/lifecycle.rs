//! The overlay's own life: the hold shield, close resetting every flag, and the listing hearing events again once closed.
use super::*;

#[gpui::test]
fn the_hold_shield_reveals_on_a_press_and_its_release_closes_nothing(cx: &mut TestAppContext) {
    // A picture reveals once its raster lands, so the held card is one still waiting on its reply.
    let (cx, seen, look, events) = host(cx, json(), Arm::Fix);
    look.update(cx, |look, cx| {
        look.close(cx);
        look.open(json(), false, cx);
    });
    cx.run_until_parked();
    events.borrow_mut().clear();
    let ground = point(px(10.), px(10.));
    cx.simulate_click(ground, Modifiers::none());
    cx.run_until_parked();
    assert!(look.read_with(cx, |look, _| look.is_open() && look.is_revealed()));
    assert_eq!(*events.borrow(), []);
    assert_eq!(*seen.borrow(), Seen::default());
    cx.simulate_click(ground, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(*events.borrow(), [Event::Closed]);
    assert_eq!(
        *seen.borrow(),
        Seen {
            hover: 1,
            ..Seen::default()
        }
    );
}

#[gpui::test]
fn an_unrevealed_wheel_reveals_and_scrolls_nothing(cx: &mut TestAppContext) {
    let (cx, seen, look, _) = host(cx, json(), Arm::Fix);
    look.update(cx, |look, cx| {
        look.close(cx);
        look.open(json(), false, cx);
    });
    let at = point(px(550.), px(350.));
    let wheel = ScrollDelta::Pixels(point(px(0.), px(-120.)));
    cx.simulate_event(ScrollWheelEvent {
        position: at,
        delta: wheel,
        modifiers: Modifiers::none(),
        touch_phase: TouchPhase::Moved,
    });
    cx.run_until_parked();
    assert!(look.read_with(cx, |look, _| look.is_revealed()));
    assert_eq!(*seen.borrow(), Seen::default());
}

#[gpui::test]
fn close_resets_the_overlay_and_hands_the_pointer_back(cx: &mut TestAppContext) {
    let (cx, seen, look, events) = host(cx, json(), Arm::Fix);
    assert_eq!(
        look.update(cx, |look, cx| look.action("expand", cx)),
        flea::ui_logic_pv1::keys::Command::ToggleExpand
    );
    let full = cx.update(|window, _| window.viewport_size());
    assert_eq!(look.read_with(cx, |look, _| look.card(full).size), full);
    look.update(cx, |look, cx| look.action("escape", cx));
    cx.run_until_parked();
    assert_eq!(*events.borrow(), [Event::Closed]);
    // a second close is no second event
    look.update(cx, |look, cx| look.close(cx));
    assert_eq!(*events.borrow(), [Event::Closed]);
    look.update(cx, |look, cx| look.open(json(), true, cx));
    assert!(look.read_with(cx, |look, _| look.card(full).size != full));
    look.update(cx, |look, cx| look.close(cx));
    cx.run_until_parked();
    // with the overlay gone the same click reaches the listing again
    cx.simulate_click(point(px(550.), px(350.)), Modifiers::none());
    cx.run_until_parked();
    let seen = *seen.borrow();
    assert_eq!((seen.cursor, seen.selection, seen.raw), (1, 1, 1));
}

#[gpui::test]
fn the_cursor_keys_follow_the_listing_and_space_closes(cx: &mut TestAppContext) {
    let (cx, _, look, events) = host(cx, json(), Arm::Fix);
    look.update(cx, |look, cx| {
        look.action("cursorDown", cx);
        look.action("cursorUp", cx);
        look.action("preview", cx);
    });
    cx.run_until_parked();
    assert_eq!(
        *events.borrow(),
        [Event::Follow(1), Event::Follow(-1), Event::Closed]
    );
}

#[gpui::test]
fn the_unsupported_card_sits_where_the_board_draws_it(cx: &mut TestAppContext) {
    let i18n = crate::test_catalogs::english();
    let (cx, _, look, _) = host(cx, can_not_show(&i18n), Arm::Fix);
    let window = cx.update(|window, _| window.viewport_size());
    let card = look.read_with(cx, |look, _| look.card(window));
    assert_eq!(
        (card.origin, card.size),
        (point(px(99.), px(63.)), size(px(902.), px(574.)))
    );
    // the text card is the tall one
    look.update(cx, |look, cx| look.open(json(), true, cx));
    let card = look.read_with(cx, |look, _| look.card(window));
    assert_eq!((card.origin.y, card.size.height), (px(24.), px(652.)));
}
