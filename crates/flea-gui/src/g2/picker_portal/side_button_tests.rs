//! Real pointer events pass through the rendered chooser root and its navigation owner.
use super::{
    process::Line,
    tests::fixture::{fixture, Fixture},
};
use gpui::{
    point, px, Modifiers, MouseButton, MouseDownEvent, NavigationDirection, TestAppContext,
};

// The backend echoes the pickerGen of the newest request for this folder; Recent's listpaths lands as "/".
pub(super) fn echo(f: &Fixture, path: &str) -> String {
    let asked = |line: &String| match flea::json::field_str(line, "c").as_deref() {
        Some("list") => flea::json::field_str(line, "path").as_deref() == Some(path),
        Some("listpaths") => path == "/",
        _ => false,
    };
    let sent = f.listing.sent();
    let generation = sent
        .iter()
        .rev()
        .find(|line| asked(line))
        .and_then(|line| flea::json::field_usize(line, "pickerGen"));
    generation.map_or_else(String::new, |generation| {
        format!(r#","pickerGen":{generation}"#)
    })
}

pub(super) fn landed(f: &mut Fixture, path: &str) {
    let tag = echo(f, path);
    f.listing.say(Line::Reply(format!(
        r#"{{"t":"listed","n":1,"path":"{path}"{tag}}}"#
    )));
    f.listing.say(Line::Reply(format!(r#"{{"t":"rows","start":0,"listing":1,"rows":[{{"n":"sub","d":true,"s":0,"m":0,"p":16877,"i":"folder"}}]{tag}}}"#)));
    f.cx.run_until_parked();
}

pub(super) fn failed(f: &mut Fixture, path: &str) {
    let tag = echo(f, path);
    f.listing.say(Line::Reply(format!(
        r#"{{"t":"error","where":"scan","path":"{path}","msg":"refused"{tag}}}"#
    )));
    f.cx.run_until_parked();
}

pub(super) fn press(f: &mut Fixture, direction: NavigationDirection) {
    let position = f.cx.debug_bounds("pk2-listing").unwrap().origin + point(px(80.), px(15.5));
    let button = MouseButton::Navigate(direction);
    f.cx.simulate_event(MouseDownEvent {
        button,
        position,
        click_count: 1,
        ..Default::default()
    });
    f.cx.simulate_mouse_up(position, button, Modifiers::none());
    f.cx.run_until_parked();
}

fn open(f: &mut Fixture, path: &str) {
    f.update(|picker, cx| picker.open_folder(path, cx));
    landed(f, path);
}

fn trail(f: &mut Fixture) -> (Vec<String>, Vec<String>) {
    f.read(|picker| (picker.chrome.history.clone(), picker.chrome.ahead.clone()))
}

#[gpui::test]
fn side_buttons_empty_history_parent_and_empty_forward(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    landed(&mut f, "/d");
    let sent = f.listing.sent().len();
    press(&mut f, NavigationDirection::Forward);
    assert_eq!(f.listing.sent().len(), sent);
    press(&mut f, NavigationDirection::Back);
    assert_eq!(f.read(|p| p.listing.location.clone()), "/");
    landed(&mut f, "/");
    let sent = f.listing.sent().len();
    press(&mut f, NavigationDirection::Back);
    assert_eq!(
        f.read(|p| p.listing.location.clone()),
        "/d",
        "parent navigation remains in Back history"
    );
    landed(&mut f, "/d");
    assert!(f.listing.sent().len() > sent);
    press(&mut f, NavigationDirection::Forward);
    landed(&mut f, "/");
    f.update(|p, _| p.chrome.history.clear());
    let sent = f.listing.sent().len();
    press(&mut f, NavigationDirection::Back);
    assert_eq!(f.listing.sent().len(), sent, "root has no parent");
}

#[gpui::test]
fn ordinary_success_clears_forward_but_failure_and_refusal_keep_it(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    landed(&mut f, "/d");
    open(&mut f, "/d/sub");
    press(&mut f, NavigationDirection::Back);
    landed(&mut f, "/d");
    let before = trail(&mut f);
    f.update(|p, cx| p.open_folder("relative", cx));
    assert_eq!(trail(&mut f), before);
    f.update(|p, cx| p.open_folder("/missing", cx));
    assert_eq!(
        trail(&mut f),
        (vec!["/d".into()], before.1.clone()),
        "leaving a confirmed folder immediately records the way back"
    );
    f.listing.say(Line::Reply(
        r#"{"t":"error","where":"scan","path":"/missing","msg":"missing"}"#.into(),
    ));
    f.cx.run_until_parked();
    assert_eq!(trail(&mut f), before);
    press(&mut f, NavigationDirection::Forward);
    landed(&mut f, "/d/sub");
    assert_eq!(
        trail(&mut f),
        (vec!["/d".into()], vec![]),
        "failed folder never enters history"
    );
    press(&mut f, NavigationDirection::Back);
    landed(&mut f, "/d");
    open(&mut f, "/other");
    assert_eq!(trail(&mut f), (vec!["/d".into()], vec![]));
    let sent = f.listing.sent().len();
    press(&mut f, NavigationDirection::Forward);
    assert_eq!(f.listing.sent().len(), sent);
}

#[gpui::test]
fn back_can_continue_while_pending_and_failed_forward_keeps_the_folder_left(
    cx: &mut TestAppContext,
) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    landed(&mut f, "/d");
    open(&mut f, "/d/sub");
    open(&mut f, "/d/sub/deep");
    press(&mut f, NavigationDirection::Back);
    assert_eq!(
        trail(&mut f),
        (vec!["/d".into()], vec!["/d/sub/deep".into()])
    );
    let sent = f.listing.sent().len();
    press(&mut f, NavigationDirection::Forward);
    assert_eq!(
        f.listing.sent().len(),
        sent,
        "Forward waits for an outstanding listing"
    );
    press(&mut f, NavigationDirection::Back);
    assert_eq!(f.read(|p| p.listing.location.clone()), "/d");
    landed(&mut f, "/d/sub");
    assert_eq!(
        f.read(|p| p.listing.state),
        "loading",
        "abandoned Back reply stays stale"
    );
    landed(&mut f, "/d");
    assert_eq!(trail(&mut f), (vec![], vec!["/d/sub/deep".into()]));
    press(&mut f, NavigationDirection::Forward);
    f.listing.say(Line::Reply(
        r#"{"t":"error","where":"scan","path":"/d/sub/deep","msg":"gone"}"#.into(),
    ));
    f.cx.run_until_parked();
    let before = trail(&mut f);
    press(&mut f, NavigationDirection::Back);
    assert_eq!(f.read(|p| p.listing.location.clone()), "/d");
    assert_eq!(
        trail(&mut f),
        before,
        "Back after failed Forward restores the source without moving either trail"
    );
    landed(&mut f, "/d");
    press(&mut f, NavigationDirection::Forward);
    landed(&mut f, "/d/sub/deep");
    assert_eq!(trail(&mut f), (vec!["/d".into()], vec![]));
}

#[gpui::test]
fn recent_round_trip_and_keyboard_back_share_the_history(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    landed(&mut f, "/d");
    let history = f.root.0.join("recently-used.xbel");
    f.update(|p, cx| {
        p.rail.history = history.to_string_lossy().into_owned();
        p.open_folder(flea::ui_logic_picker_portal::RECENT, cx);
    });
    f.cx.run_until_parked();
    landed(&mut f, "/");
    open(&mut f, "/d/sub");
    let focus = f.read(|p| p.focus.clone());
    f.cx.update(|w, cx| w.focus(&focus, cx));
    f.cx.simulate_keystrokes("shift-h->H");
    f.cx.run_until_parked();
    assert_eq!(
        f.read(|p| p.listing.location.clone()),
        flea::ui_logic_picker_portal::RECENT
    );
    assert!(f.listing.sent().last().unwrap().contains("listpaths"));
    landed(&mut f, "/");
    press(&mut f, NavigationDirection::Forward);
    landed(&mut f, "/d/sub");
    assert_eq!(
        trail(&mut f).0,
        ["/d", flea::ui_logic_picker_portal::RECENT]
    );
    press(&mut f, NavigationDirection::Back);
    landed(&mut f, "/");
    press(&mut f, NavigationDirection::Back);
    landed(&mut f, "/d");
    press(&mut f, NavigationDirection::Forward);
    assert_eq!(
        f.read(|p| p.listing.location.clone()),
        flea::ui_logic_picker_portal::RECENT
    );
    landed(&mut f, "/");
}

#[gpui::test]
fn side_buttons_rendered_root_round_trip(cx: &mut TestAppContext) {
    for save in [false, true] {
        let request = if save {
            r#"{"mode":"save","name":"a.txt","folder":"/d"}"#
        } else {
            r#"{"folder":"/d"}"#
        };
        let mut f = fixture(cx, request, None);
        landed(&mut f, "/d");
        f.update(|picker, cx| picker.open_folder("/d/sub", cx));
        landed(&mut f, "/d/sub");
        press(&mut f, NavigationDirection::Back);
        assert_eq!(
            f.read(|picker| picker.listing.location.clone()),
            "/d",
            "Back must reach the picker owner, save={save}"
        );
        landed(&mut f, "/d");
        press(&mut f, NavigationDirection::Forward);
        assert_eq!(
            f.read(|picker| picker.listing.location.clone()),
            "/d/sub",
            "Forward retraces Back, save={save}"
        );
        landed(&mut f, "/d/sub");
        assert_eq!(f.read(|picker| picker.listing.cursor), 0);
        if save {
            assert_eq!(f.read(|picker| picker.listing.save_name.clone()), "a.txt");
        }
        assert_eq!(f.written(), None, "side buttons never accept the row below");
    }
}

#[gpui::test]
fn superseded_and_stale_destinations_never_enter_history(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    landed(&mut f, "/d");
    open(&mut f, "/b");
    press(&mut f, NavigationDirection::Back);
    f.update(|p, cx| p.open_folder("/c", cx));
    landed(&mut f, "/d");
    assert_eq!(
        trail(&mut f),
        (vec!["/d".into(), "/b".into()], vec![]),
        "stale Back has no effect"
    );
    landed(&mut f, "/c");
    assert_eq!(trail(&mut f), (vec!["/d".into(), "/b".into()], vec![]));
    press(&mut f, NavigationDirection::Back);
    landed(&mut f, "/b");
    open(&mut f, "/b");
    assert_eq!(
        trail(&mut f),
        (vec!["/d".into()], vec!["/c".into()]),
        "refresh retains the trail"
    );
}
