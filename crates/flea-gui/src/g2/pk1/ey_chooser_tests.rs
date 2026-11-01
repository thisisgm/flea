// Chooser defects from the eyes sweep, each held to what the v0.3.12 arm does in the same steps.
use super::process::Line;
use super::tests::fixture::{fixture, Fixture};
use gpui::{KeyUpEvent, Keystroke, TestAppContext};

fn seed(f: &mut Fixture) {
    f.listing.say(Line::Reply(
        r#"{"t":"listed","n":2,"v":1,"w":true,"path":"/d"}"#.into(),
    ));
    f.listing.say(Line::Reply(r#"{"t":"rows","start":0,"listing":1,"rows":[{"n":"a.txt","d":false,"s":3,"m":0,"p":33188,"i":"text-x-generic"},{"n":"b.md","d":false,"s":7,"m":0,"p":33188,"i":"text-x-generic"}]}"#.into()));
    f.cx.run_until_parked();
    let focus = f.read(|picker| picker.focus.clone());
    f.cx.update(|window, cx| window.focus(&focus, cx));
    f.cx.run_until_parked();
}

// A real key is pressed and released; gpui's keyboard click fires on the release of Enter or Space.
fn press(f: &mut Fixture, key: &str) {
    f.cx.simulate_keystrokes(key);
    f.cx.simulate_event(KeyUpEvent {
        keystroke: Keystroke::parse(key).unwrap(),
    });
    f.cx.run_until_parked();
}

#[gpui::test]
fn ey500_a_keyboard_pick_closes_the_filter_menu(cx: &mut TestAppContext) {
    for opener in ["enter", "space", "down"] {
        let mut f = fixture(
            cx,
            r#"{"folder":"/d","filters":[{"label":"Text","globs":["*.txt"]},{"label":"Markdown","globs":["*.md"]}]}"#,
            None,
        );
        f.cx.run_until_parked();
        seed(&mut f);
        press(&mut f, "tab");
        press(&mut f, opener);
        assert!(
            f.cx.debug_bounds("picker-filter-menu").is_some(),
            "{opener} on the trigger opens the Filter menu"
        );
        press(&mut f, "down");
        press(&mut f, "enter");
        assert_eq!(f.read(|picker| picker.listing.chip), 1, "{opener}");
        assert!(
            f.read(|picker| picker.chrome.menu_view.is_none()),
            "{opener}: the pick closes the menu"
        );
        assert!(
            f.cx.debug_bounds("picker-filter-menu").is_none(),
            "{opener}: no Filter menu is drawn after the pick"
        );
    }
}
