// "/" in the chooser on the production Picker: real keys, the listing child's lines, the filter answer and its strip (#312, #319).
use super::*;
use gpui::{KeyDownEvent, Keystroke, Modifiers};

fn listed(f: &mut Fixture, names: &[&str]) {
    let rows: Vec<String> = names
        .iter()
        .map(|name| {
            format!(r#"{{"n":"{name}","d":false,"s":3,"m":0,"p":33188,"i":"text-x-generic"}}"#)
        })
        .collect();
    f.listing.say(Line::Reply(format!(
        r#"{{"t":"listed","n":{},"read":0.000,"sort":0.000,"v":1,"w":true,"path":"/d"}}"#,
        names.len()
    )));
    f.listing.say(Line::Reply(format!(
        r#"{{"t":"rows","start":0,"rows":[{}],"listing":1}}"#,
        rows.join(",")
    )));
    f.cx.run_until_parked();
}

fn press(f: &mut Fixture, key: &str, text: Option<&str>) {
    if let Some(text) = text {
        if text != "/" && f.read(|picker| picker.find.owner.typing()) {
            f.cx.simulate_input(text);
            f.cx.run_until_parked();
            return;
        }
    }
    let keystroke = Keystroke {
        key: key.into(),
        key_char: text.map(Into::into),
        modifiers: Modifiers::default(),
        ..Default::default()
    };
    let event = KeyDownEvent {
        keystroke,
        is_held: false,
        prefer_character_input: false,
    };
    let picker = f.picker.clone();
    f.cx.update(|window, cx| picker.update(cx, |picker, cx| picker.key(&event, window, cx)));
    f.cx.run_until_parked();
}

fn answer(f: &mut Fixture, id: u64, query: &str, rows: &str) {
    f.listing.say(Line::Reply(format!(
        r#"{{"t":"filtered","id":{id},"query":"{query}","n":4,"rows":[{rows}],"ms":0.010,"listing":1}}"#
    )));
    f.cx.run_until_parked();
}

fn strip(f: &mut Fixture) -> Option<(String, bool, String)> {
    f.read(|picker| {
        picker.find.owner.up().then(|| {
            let shown = picker.listing.shown_total;
            (
                picker.find.owner.query().to_owned(),
                picker.find.owner.typing(),
                shown.to_string(),
            )
        })
    })
}

#[gpui::test]
fn slash_filters_the_whole_listing_and_enter_opens_the_single_match(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    f.cx.run_until_parked();
    listed(&mut f, &["alpha.txt", "notes.txt", "nuts.txt", "zeta.md"]);
    let before = f.listing.sent().len();
    press(&mut f, "/", Some("/"));
    assert_eq!(
        strip(&mut f),
        Some((String::new(), true, "4".into())),
        "/ opens the filter line with its caret"
    );
    assert_eq!(
        f.listing.sent().len(),
        before,
        "nothing is asked until a key is typed"
    );
    press(&mut f, "n", Some("n"));
    assert_eq!(
        f.listing.sent().last().map(String::as_str),
        Some(r#"{"c":"filter","query":"n","id":1}"#),
        "the typed key asks the listing child for the whole folder"
    );
    answer(&mut f, 1, "n", "1,2");
    assert_eq!(strip(&mut f), Some(("n".into(), true, "2".into())));
    assert_eq!(
        f.read(|picker| picker.listing.cursor),
        1,
        "the cursor takes the first match"
    );
    assert_eq!(
        f.read(|picker| picker.hints().to_string()),
        "enter open \u{b7} esc clears"
    );
    let drawn = f.read(|picker| picker.browse.listing.clone());
    assert_eq!(
        f.cx.update(|_, cx| drawn.read(cx).shown().map(<[usize]>::to_vec)),
        Some(vec![1, 2]),
        "the list draws only the matches"
    );
    press(&mut f, "o", Some("o"));
    answer(&mut f, 2, "no", "1");
    press(&mut f, "enter", None);
    assert_eq!(
        f.checks.sent(),
        [
            r#"{"op":"mark","path":"/d/notes.txt","directory":false,"multiple":false,"c":"picker","id":1}"#
        ],
        "Enter on the line opens the one exact match"
    );
}

#[gpui::test]
fn escape_clears_the_filter_first_and_a_second_escape_cancels(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    f.cx.run_until_parked();
    listed(&mut f, &["alpha.txt", "notes.txt", "nuts.txt", "zeta.md"]);
    press(&mut f, "/", Some("/"));
    press(&mut f, "n", Some("n"));
    answer(&mut f, 1, "n", "1,2");
    press(&mut f, "enter", None);
    assert_eq!(
        strip(&mut f),
        Some(("n".into(), false, "2".into())),
        "Enter commits the line, the filter stands"
    );
    press(&mut f, "down", None);
    assert_eq!(
        f.read(|picker| picker.listing.cursor),
        2,
        "a step goes to the next match, not the next row"
    );
    press(&mut f, "escape", None);
    assert_eq!(strip(&mut f), None, "the first esc clears the filter");
    assert_eq!(f.read(|picker| picker.listing.shown_total), 4);
    assert!(f.written().is_none(), "and does not cancel");
    press(&mut f, "escape", None);
    f.settle(|f| f.written().is_some());
    assert_eq!(f.written().as_deref(), Some(r#"{"response":1}"#));
}

fn chord(f: &mut Fixture, key: &str) {
    let keystroke = Keystroke {
        key: key.into(),
        modifiers: Modifiers {
            control: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let event = KeyDownEvent {
        keystroke,
        is_held: false,
        prefer_character_input: false,
    };
    let picker = f.picker.clone();
    f.cx.update(|window, cx| picker.update(cx, |picker, cx| picker.key(&event, window, cx)));
    f.cx.run_until_parked();
}

#[gpui::test]
fn a_key_the_line_declines_returns_once_and_leaves_the_caret(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    f.cx.run_until_parked();
    // The matches sit apart and off row 0, so neither a step nor a leaked key looks like the list's own move.
    listed(&mut f, &["alpha.txt", "notes.txt", "zeta.md", "nuts.txt"]);
    press(&mut f, "/", Some("/"));
    press(&mut f, "n", Some("n"));
    answer(&mut f, 1, "n", "1,3");
    press(&mut f, "tab", None);
    press(&mut f, "f5", None);
    chord(&mut f, "k");
    assert_eq!(
        strip(&mut f),
        Some(("n".into(), true, "2".into())),
        "Tab, an F-key and an unbound chord are consumed on the line and keep its caret"
    );
    assert_eq!(f.read(|picker| picker.listing.cursor), 1);
    press(&mut f, "down", None);
    assert_eq!(
        strip(&mut f),
        Some(("n".into(), false, "2".into())),
        "a key that leaves the line commits it first"
    );
    assert_eq!(
        f.read(|picker| picker.listing.cursor),
        3,
        "and the same key's second pass steps to the next match"
    );
}

#[gpui::test]
fn a_cursor_key_on_an_empty_line_closes_it_and_returns(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    f.cx.run_until_parked();
    listed(&mut f, &["alpha.txt", "notes.txt"]);
    press(&mut f, "/", Some("/"));
    assert_eq!(strip(&mut f), Some((String::new(), true, "2".into())));
    press(&mut f, "down", None);
    assert_eq!(
        strip(&mut f),
        None,
        "an empty line has nothing to stand, so it closes"
    );
    assert_eq!(
        f.read(|picker| picker.listing.cursor),
        1,
        "and the key still moves the list, which owns it once nothing is drawn"
    );
}

#[gpui::test]
fn the_shown_count_is_the_backend_total_after_the_type_chip(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"folder":"/d","filters":[{"label":"Text","globs":["*.txt"]}]}"#,
        None,
    );
    f.cx.run_until_parked();
    assert!(
        f.listing
            .sent()
            .iter()
            .any(|line| line.contains(r#""pickerGlobs":["*.txt"]"#)),
        "the chip narrows on the backend: {:?}",
        f.listing.sent()
    );
    listed(&mut f, &["a.txt", "b.txt"]);
    assert_eq!(f.read(|picker| picker.listing.shown_total), 2);
    press(&mut f, "/", Some("/"));
    press(&mut f, "a", Some("a"));
    answer(&mut f, 1, "a", "0");
    assert_eq!(
        f.read(|picker| picker.listing.shown_total),
        1,
        "a find under the chip counts its own rows within the chip's"
    );
    press(&mut f, "escape", None);
    assert_eq!(f.read(|picker| picker.listing.shown_total), 2);
}
