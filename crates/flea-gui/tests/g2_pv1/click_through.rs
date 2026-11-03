//! #331: a click, double click, right click, wheel or move on Quick Look never reaches the listing behind it.
use super::*;
use gpui::Bounds;

fn card(look: &Entity<QuickLook>, cx: &mut VisualTestContext) -> Bounds<Pixels> {
    let window = cx.update(|window, _| window.viewport_size());
    look.read_with(cx, |look, _| look.card(window))
}

/// Points inside the preview: the bar, the middle of the content and a content corner.
fn inside(card: Bounds<Pixels>) -> [Point<Pixels>; 3] {
    let o = card.origin;
    [
        o + point(card.size.width / 2., px(13.)),
        card.center(),
        o + point(px(20.), card.size.height - px(20.)),
    ]
}

fn kept(cx: &mut TestAppContext, subject: Subject) {
    let (cx, seen, look, events) = host(cx, subject, Arm::Fix);
    let card = card(&look, cx);
    for at in inside(card) {
        gestures(cx, at);
    }
    assert_eq!(*seen.borrow(), Seen::default());
    assert!(look.read_with(cx, |look, _| look.is_open()));
    assert_eq!(*events.borrow(), []);
}

fn leaked(cx: &mut TestAppContext, subject: Subject) {
    let (cx, seen, look, _) = host(cx, subject, Arm::OracleCopy);
    for at in inside(card(&look, cx)) {
        gestures(cx, at);
    }
    let seen = *seen.borrow();
    // Each counter moving is the issue as filed: the cursor, the selection, the menu, the scroll and the hover row.
    assert!(
        seen.cursor > 0 && seen.selection > 0 && seen.menu > 0,
        "{seen:?}"
    );
    assert!(
        seen.scroll > 0 && seen.hover > 0 && seen.raw > 0,
        "{seen:?}"
    );
}

#[gpui::test]
fn a_json_preview_keeps_every_pointer_event(cx: &mut TestAppContext) {
    kept(cx, json());
}

#[gpui::test]
fn an_image_preview_keeps_every_pointer_event(cx: &mut TestAppContext) {
    kept(cx, image());
}

#[gpui::test]
fn the_oracle_copy_lets_a_json_previews_events_through(cx: &mut TestAppContext) {
    leaked(cx, json());
}

#[gpui::test]
fn the_oracle_copy_lets_an_image_previews_events_through(cx: &mut TestAppContext) {
    leaked(cx, image());
}

#[gpui::test]
fn the_ground_only_closes_on_a_left_click_and_never_passes_one_on(cx: &mut TestAppContext) {
    let (cx, seen, look, events) = host(cx, json(), Arm::Fix);
    let card = card(&look, cx);
    let ground = point(card.origin.x / 2., px(350.));
    assert!(!card.contains(&ground));
    let none = Modifiers::none();
    cx.simulate_mouse_move(ground, None, none);
    cx.simulate_mouse_down(ground, MouseButton::Right, none);
    cx.simulate_mouse_up(ground, MouseButton::Right, none);
    let wheel = ScrollDelta::Pixels(point(px(0.), px(-120.)));
    cx.simulate_event(ScrollWheelEvent {
        position: ground,
        delta: wheel,
        modifiers: none,
        touch_phase: TouchPhase::Moved,
    });
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: ground,
        modifiers: none,
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: ground,
        modifiers: none,
        click_count: 2,
    });
    cx.run_until_parked();
    assert!(look.read_with(cx, |look, _| look.is_open()));
    assert_eq!(*seen.borrow(), Seen::default());
    cx.simulate_click(ground, none);
    cx.run_until_parked();
    assert!(!look.read_with(cx, |look, _| look.is_open()));
    assert_eq!(*events.borrow(), [Event::Closed]);
    // the closing click's release clicks nothing behind it; only the hover row returns under the pointer
    assert_eq!(
        *seen.borrow(),
        Seen {
            hover: 1,
            ..Seen::default()
        }
    );
}

#[gpui::test]
fn open_with_answers_inside_the_card_and_the_listing_hears_nothing(cx: &mut TestAppContext) {
    let i18n = crate::test_catalogs::english();
    let (cx, seen, look, events) = host(cx, can_not_show(&i18n), Arm::Fix);
    let card = card(&look, cx);
    let button = cx
        .debug_bounds("pv1-open-with")
        .expect("the Open With button draws");
    assert!(card.contains(&button.center()));
    cx.simulate_click(button.center(), Modifiers::none());
    cx.run_until_parked();
    assert_eq!(*events.borrow(), [Event::OpenWith]);
    assert_eq!(*seen.borrow(), Seen::default());
}
