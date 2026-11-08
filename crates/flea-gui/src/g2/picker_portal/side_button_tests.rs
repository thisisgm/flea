//! The mouse side buttons in the chooser follow the main window: Back goes back or up a folder, Forward retraces a Back.
use super::process::Line;
use super::tests::fixture::{fixture, Fixture};
use gpui::{
    point, px, Modifiers, MouseButton, MouseDownEvent, NavigationDirection, TestAppContext,
};

// Open File on /d with a file and the folder sub, in Single Click mode so one click opens sub.
fn seed(cx: &mut TestAppContext) -> Fixture<'_> {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    let settings = flea::jsondoc::parse(r#"{"openMode":"single"}"#).unwrap();
    f.update(|picker, cx| picker.apply_settings(&settings, cx));
    f.listing
        .say(Line::Reply(r#"{"t":"listed","n":2,"path":"/d"}"#.into()));
    f.listing.say(Line::Reply(r#"{"t":"rows","start":0,"listing":1,"rows":[{"n":"a.txt","d":false,"s":3,"m":0,"p":33188,"i":"text-x-generic"},{"n":"sub","d":true,"s":0,"m":0,"p":16877,"i":"folder"}]}"#.into()));
    f.cx.run_until_parked();
    let focus = f.read(|picker| picker.focus.clone());
    f.cx.update(|window, cx| window.focus(&focus, cx));
    f
}

fn open_sub(f: &mut Fixture) {
    let bounds = f.cx.debug_bounds("pk2-listing").unwrap();
    f.cx.simulate_click(
        bounds.origin + point(px(80.), px(31. + 15.5)),
        Modifiers::none(),
    );
    f.cx.run_until_parked();
    assert_eq!(location(f), "/d/sub");
}

fn press(f: &mut Fixture, direction: NavigationDirection) {
    let bounds = f.cx.debug_bounds("pk2-listing").unwrap();
    let (position, button) = (
        bounds.origin + point(px(80.), px(15.5)),
        MouseButton::Navigate(direction),
    );
    f.cx.simulate_event(MouseDownEvent {
        button,
        position,
        click_count: 1,
        ..Default::default()
    });
    f.cx.simulate_mouse_up(position, button, Modifiers::none());
    f.cx.run_until_parked();
}

fn location(f: &mut Fixture) -> String {
    f.read(|picker| picker.listing.location.clone())
}

fn ahead(f: &mut Fixture) -> Vec<String> {
    f.read(|picker| picker.chrome.ahead.clone())
}

#[gpui::test]
fn side_back_then_forward_round_trips_a_folder(cx: &mut TestAppContext) {
    let mut f = seed(cx);
    open_sub(&mut f);
    press(&mut f, NavigationDirection::Back);
    assert_eq!(
        location(&mut f),
        "/d",
        "Back returns to the folder sub was opened from"
    );
    assert_eq!(ahead(&mut f), ["/d/sub"]);
    press(&mut f, NavigationDirection::Forward);
    assert_eq!(location(&mut f), "/d/sub", "Forward retraces the Back");
    assert!(ahead(&mut f).is_empty());
    assert_eq!(f.read(|picker| picker.chrome.history.clone()), ["/d"]);
}

#[gpui::test]
fn side_back_with_no_history_climbs_to_the_parent(cx: &mut TestAppContext) {
    let mut f = seed(cx);
    assert!(f.read(|picker| picker.chrome.history.is_empty()));
    press(&mut f, NavigationDirection::Back);
    assert_eq!(
        location(&mut f),
        "/",
        "as in the main window, Back climbs when there is no history"
    );
}

#[gpui::test]
fn side_forward_with_nothing_ahead_stays_put(cx: &mut TestAppContext) {
    let mut f = seed(cx);
    let sent = f.listing.sent().len();
    press(&mut f, NavigationDirection::Forward);
    assert_eq!(location(&mut f), "/d");
    assert_eq!(f.listing.sent().len(), sent, "no listing request");
}

#[gpui::test]
fn a_new_folder_after_a_back_clears_forward(cx: &mut TestAppContext) {
    let mut f = seed(cx);
    open_sub(&mut f);
    press(&mut f, NavigationDirection::Back);
    assert_eq!(ahead(&mut f), ["/d/sub"]);
    f.update(|picker, cx| picker.open_folder("/tmp", cx));
    assert!(
        ahead(&mut f).is_empty(),
        "opening another folder ends the forward trail"
    );
    press(&mut f, NavigationDirection::Forward);
    assert_eq!(location(&mut f), "/tmp");
}

#[gpui::test]
fn side_buttons_are_refused_behind_the_filter_menu(cx: &mut TestAppContext) {
    let mut f = seed(cx);
    open_sub(&mut f);
    f.update(|picker, _| picker.chrome.menu = Some(0));
    press(&mut f, NavigationDirection::Back);
    assert_eq!(location(&mut f), "/d/sub");
}
