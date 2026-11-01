//! #372: with Settings > View "Open items with" set to Single Click, one click on a chooser row activates it.
use super::process::Line;
use super::tests::fixture::{fixture, Fixture};
use gpui::{point, px, Modifiers, MouseButton, MouseDownEvent, TestAppContext};

fn seed(f: &mut Fixture, open_mode: &str) {
    let settings = flea::jsondoc::parse(&format!(r#"{{"openMode":"{open_mode}"}}"#)).unwrap();
    f.update(|picker, cx| picker.apply_settings(&settings, cx));
    f.listing
        .say(Line::Reply(r#"{"t":"listed","n":3,"path":"/d"}"#.into()));
    f.listing.say(Line::Reply(r#"{"t":"rows","start":0,"listing":1,"rows":[{"n":"a.txt","d":false,"s":3,"m":0,"p":33188,"i":"text-x-generic"},{"n":"b.txt","d":false,"s":7,"m":0,"p":33188,"i":"text-x-generic"},{"n":"sub","d":true,"s":0,"m":0,"p":16877,"i":"folder"}]}"#.into()));
    f.cx.run_until_parked();
    let focus = f.read(|picker| picker.focus.clone());
    f.cx.update(|window, cx| window.focus(&focus, cx));
}

fn click(f: &mut Fixture, row: usize) {
    let bounds = f.cx.debug_bounds("pk2-listing").unwrap();
    let at = bounds.origin + point(px(80.), px(row as f32 * 31. + 15.5));
    f.cx.simulate_click(at, Modifiers::none());
    f.cx.run_until_parked();
}

fn mark_sent(f: &Fixture, path: &str) -> bool {
    f.checks
        .sent()
        .iter()
        .any(|line| line.contains(path) && line.contains("\"op\":\"mark\""))
}

#[gpui::test]
fn ey372_single_click_opens_a_folder(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    seed(&mut f, "single");
    click(&mut f, 2);
    assert_eq!(f.read(|picker| picker.listing.location.clone()), "/d/sub");
}

#[gpui::test]
fn ey372_single_click_asks_to_accept_a_file(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    seed(&mut f, "single");
    click(&mut f, 1);
    assert_eq!(f.read(|picker| picker.listing.cursor), 1);
    assert!(f.read(|picker| picker.accept_marks));
    assert!(mark_sent(&f, "/d/b.txt"));
}

#[gpui::test]
fn ey372_single_click_marks_in_a_multiple_chooser(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d","multiple":true}"#, None);
    seed(&mut f, "single");
    click(&mut f, 1);
    assert!(mark_sent(&f, "/d/b.txt"));
    assert!(!f.read(|picker| picker.accept_marks));
}

#[gpui::test]
fn ey372_double_click_setting_keeps_one_click_as_a_cursor_move(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    seed(&mut f, "double");
    click(&mut f, 2);
    assert_eq!(f.read(|picker| picker.listing.location.clone()), "/d");
    assert_eq!(f.read(|picker| picker.listing.cursor), 2);
    assert!(f.checks.sent().is_empty());
}

#[gpui::test]
fn ey372_rapid_clicks_activate_each_different_row_once(cx: &mut TestAppContext) {
    for second_row in [0, 1] {
        let mut f = fixture(cx, r#"{"folder":"/d","multiple":true}"#, None);
        seed(&mut f, "single");
        let bounds = f.cx.debug_bounds("pk2-listing").unwrap();
        let row = f.update(|picker, cx| picker.browse.listing.read(cx).drawn().file_metrics().row);
        let press = |f: &mut Fixture, y, count| {
            let at = bounds.origin + point(px(80.), px(y));
            f.cx.simulate_event(MouseDownEvent {
                button: MouseButton::Left,
                position: at,
                click_count: count,
                ..Default::default()
            });
            f.cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
            f.cx.run_until_parked();
        };
        press(&mut f, row - 1., 1);
        assert!(mark_sent(&f, "/d/a.txt"));
        let id = f.read(|picker| picker.mark_request);
        f.checks.say(Line::Reply(format!(r#"{{"t":"picker","id":{id},"op":"mark","ok":true,"marks":[{{"path":"/d/a.txt","bytes":3}}],"removed":0}}"#)));
        f.cx.run_until_parked();
        assert_eq!(f.read(|picker| picker.mark_request), 0);
        let sent = f.checks.sent().len();
        press(&mut f, if second_row == 0 { row - 1. } else { row + 1. }, 2);
        if second_row == 1 {
            assert!(
                mark_sent(&f, "/d/b.txt"),
                "a nearby click on another row activates it even when the platform counts two"
            );
        } else {
            assert_eq!(
                f.checks.sent().len(),
                sent,
                "a double-click on the same row never toggles it twice"
            );
        }
    }
}
