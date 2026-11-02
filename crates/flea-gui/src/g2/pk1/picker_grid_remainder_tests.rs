//! EY-272: literal H and Ctrl+L enter through the picker's real key owner in Grid.
use super::super::{
    ey313_tests::draw,
    process::Line,
    tests::fixture::{fixture, Fixture},
};
use gpui::{KeyDownEvent, Keystroke, TestAppContext};

const ROOT: &str = "/d/PickerGridKeys";
const ROUTE: &str = "/d/PickerGridKeys/route";
const DEST: &str = "/d/A/full";

fn listed(f: &mut Fixture, path: &str, version: usize, rows: &[(&str, bool)]) {
    let rows: Vec<String> = rows
        .iter()
        .map(|(name, directory)| {
            format!(r#"{{"n":"{name}","d":{directory},"s":1,"m":0,"p":33188,"i":"text-plain"}}"#)
        })
        .collect();
    f.listing.say(Line::Reply(format!(
        r#"{{"t":"listed","path":"{path}","n":{},"v":{version},"w":true}}"#,
        rows.len()
    )));
    f.listing.say(Line::Reply(format!(
        r#"{{"t":"rows","start":0,"listing":{version},"rows":[{}]}}"#,
        rows.join(",")
    )));
    f.cx.run_until_parked();
    draw(f);
}

fn grid(cx: &mut TestAppContext) -> Fixture<'_> {
    let mut f = fixture(cx, r#"{"folder":"/d/PickerGridKeys"}"#, None);
    f.cx.run_until_parked();
    listed(&mut f, ROOT, 1, &[("route", true), ("grid-01.txt", false)]);
    let focus = f.read(|picker| picker.focus.clone());
    f.cx.update(|window, cx| window.focus(&focus, cx));
    f.cx.simulate_keystrokes("ctrl-3");
    draw(&mut f);
    assert_eq!(
        f.update(|picker, cx| picker.browse.listing.read(cx).view_mode().to_owned()),
        "grid"
    );
    f
}

fn assert_list_request(f: &Fixture, from: usize, path: &str) {
    let expected = format!(r#""path":"{path}""#);
    assert!(
        f.listing.sent()[from..]
            .iter()
            .any(|line| line.contains(r#""c":"list""#) && line.contains(&expected)),
        "key must list {path}, sent {:?}",
        &f.listing.sent()[from..]
    );
}

#[gpui::test]
fn sol_picker_grid_remainder_literal_h_returns_to_the_visited_child(cx: &mut TestAppContext) {
    let mut f = grid(cx);
    let from = f.listing.sent().len();
    f.cx.simulate_keystrokes("enter");
    assert_list_request(&f, from, ROUTE);
    listed(&mut f, ROUTE, 2, &[("inside.txt", false)]);
    let from = f.listing.sent().len();
    f.cx.simulate_keystrokes("backspace");
    assert_list_request(&f, from, ROOT);
    listed(&mut f, ROOT, 3, &[("route", true), ("grid-01.txt", false)]);
    let from = f.listing.sent().len();
    // wtype H can supply uppercase text without Shift through its virtual keymap.
    let event = KeyDownEvent {
        keystroke: Keystroke {
            key: "h".into(),
            key_char: Some("H".into()),
            ..Default::default()
        },
        is_held: false,
        prefer_character_input: false,
    };
    let picker = f.picker.clone();
    f.cx.update(|window, cx| picker.update(cx, |picker, cx| picker.key(&event, window, cx)));
    assert_list_request(&f, from, ROUTE);
    listed(&mut f, ROUTE, 4, &[("inside.txt", false)]);
    assert_eq!(f.read(|picker| picker.listing.location.clone()), ROUTE);
    assert_eq!(
        f.read(|picker| picker.listing.cursor_file.clone())
            .as_deref(),
        Some("inside.txt")
    );
    assert!(!f.read(|picker| picker.find.owner.up()));
}

#[gpui::test]
fn sol_picker_grid_remainder_ctrl_l_navigates_and_accepts_an_absolute_path(
    cx: &mut TestAppContext,
) {
    let mut f = grid(cx);
    let cursor = f.read(|picker| picker.listing.cursor);
    f.cx.simulate_keystrokes("ctrl-l");
    let path = f.update(|picker, cx| picker.browse.path_view(cx));
    assert!(
        f.cx.update(|window, cx| path.read(cx).holds_keyboard(window)),
        "Ctrl+L opens the actual path entry from a fresh Grid chooser"
    );
    assert_eq!(f.read(|picker| picker.listing.cursor), cursor);
    assert!(!f.read(|picker| picker.find.owner.up()));
    f.cx.simulate_keystrokes("ctrl-a");
    f.cx.simulate_input(DEST);
    let from = f.listing.sent().len();
    f.cx.simulate_keystrokes("enter");
    f.cx.run_until_parked();
    assert!(f.listing.sent()[from..].iter().any(|line| {
        line.contains(r#""c":"peek""#) && line.contains(&format!(r#""path":"{DEST}""#))
    }));
    f.listing.say(Line::Reply(format!(
        r#"{{"t":"peeked","path":"{DEST}","hidden":0,"hiddenLast":false,"first":0,"n":0,"v":1,"w":true,"mode":16877,"rows":[]}}"#
    )));
    f.cx.run_until_parked();
    assert_list_request(&f, from, DEST);
    listed(&mut f, DEST, 2, &[("f1.txt", false), ("f2.txt", false)]);
    assert_eq!(f.read(|picker| picker.listing.location.clone()), DEST);
    assert_eq!(
        f.update(|picker, cx| picker.browse.listing.read(cx).view_mode().to_owned()),
        "grid"
    );
    f.cx.simulate_keystrokes("enter");
    f.cx.run_until_parked();
    let sent = f.checks.sent();
    let mark = sent.last().expect("Enter checks the destination file");
    assert!(mark.contains(r#""op":"mark""#) && mark.contains("/d/A/full/f1.txt"));
    let id = flea::json::field_usize(mark, "id").unwrap();
    f.checks.say(Line::Reply(format!(
        r#"{{"t":"picker","id":{id},"op":"mark","ok":true,"marks":[{{"path":"/d/A/full/f1.txt","bytes":1}}],"removed":0}}"#
    )));
    f.settle(|f| f.written().is_some());
    assert_eq!(
        f.written().as_deref(),
        Some(r#"{"response":0,"uris":["file:///d/A/full/f1.txt"],"filter":-1}"#)
    );
}
