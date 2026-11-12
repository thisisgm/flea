// The production Picker on its listing child: no test subscriber, only the child's lines and real keystrokes.
use super::*;

// Opening and each typed query ask Jump (provisional, then whole); each ask is answered empty under its own id,
// so Enter commits the typed line instead of waiting on the jump.
fn answer_jumps(f: &mut Fixture, from: usize) {
    let mut at = from;
    while let Some(line) = f.listing.sent().get(at).cloned() {
        at += 1;
        let Some(rest) = line.strip_prefix(r#"{"c":"jump","id":"#) else {
            continue;
        };
        let id = rest.split(',').next().unwrap_or_default();
        f.listing.say(Line::Reply(format!(
            r#"{{"t":"jumped","id":{id},"favourites":[],"zoxide":[],"recent":[],"frecency":{{}}}}"#
        )));
        f.cx.run_until_parked();
    }
}

fn row(name: &str, dir: bool) -> String {
    format!(
        r#"{{"n":"{name}","d":{dir},"s":3,"m":0,"p":{},"i":"text-x-generic"}}"#,
        if dir { 16877 } else { 33188 }
    )
}

fn listed(f: &mut Fixture, path: &str, rows: &[(&str, bool)]) {
    let drawn: Vec<String> = rows.iter().map(|(name, dir)| row(name, *dir)).collect();
    f.listing.say(Line::Reply(format!(
        r#"{{"t":"listed","n":{},"read":0.000,"sort":0.000,"v":1,"w":true,"path":"{path}"}}"#,
        rows.len()
    )));
    f.listing.say(Line::Reply(format!(
        r#"{{"t":"rows","start":0,"rows":[{}],"listing":1}}"#,
        drawn.join(",")
    )));
    f.cx.run_until_parked();
}

fn focus_list(f: &mut Fixture) {
    let focus = f.read(|picker| picker.focus.clone());
    f.cx.update(|window, cx| window.focus(&focus, cx));
    f.cx.run_until_parked();
}

fn filename_state(f: &mut Fixture) -> (String, std::ops::Range<usize>, bool) {
    let filename = f
        .read(|picker| picker.browse.filename.clone())
        .expect("a save has Filename");
    f.cx.update(|window, cx| {
        let field = filename.read(cx);
        (
            field.text().to_owned(),
            field.entry.selected.clone(),
            field.focus.is_focused(window),
        )
    })
}

#[gpui::test]
fn m9_integ_picker_save_enter_transfers_name_and_selects_stem(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"mode":"save","name":"untitled.txt","folder":"/d"}"#,
        None,
    );
    f.cx.run_until_parked();
    assert_eq!(
        f.listing.sent(),
        [
            r#"{"c":"list","path":"/d","first":128,"pickerGlobs":[],"pickerMimes":[],"hidden":false,"pickerGen":1}"#
        ],
        "the start folder is asked of the listing child"
    );
    assert_eq!(
        filename_state(&mut f),
        ("untitled.txt".into(), 0..8, true),
        "a save opens on Filename with its stem selected"
    );
    listed(&mut f, "/d", &[("report.txt", false), ("sub", true)]);
    assert_eq!(
        f.read(|picker| (picker.listing.state, picker.listing.names.clone())),
        ("ready", vec!["report.txt".to_owned(), "sub".to_owned()])
    );
    assert_eq!(
        f.checks.sent(),
        [r#"{"op":"save","folder":"/d","name":"untitled.txt","c":"picker","id":1}"#],
        "the supplied name is probed once the start folder lands"
    );
    f.checks.say(Line::Reply(
        r#"{"t":"picker","id":1,"op":"save","ok":true,"review":1,"path":"/d/untitled.txt","collision":false}"#.into(),
    ));
    f.settle(|f| f.read(|picker| picker.accept_state().save_ready()));
    focus_list(&mut f);
    f.cx.simulate_keystrokes("enter");
    f.cx.run_until_parked();
    assert_eq!(
        f.read(|picker| picker.listing.save_name.clone()),
        "report.txt",
        "Enter on a file row names the save"
    );
    assert_eq!(
        filename_state(&mut f),
        ("report.txt".into(), 0..6, true),
        "Filename takes the keyboard with the stem selected"
    );
    assert_eq!(
        f.checks.sent()[1..],
        [r#"{"op":"save","folder":"/d","name":"report.txt","c":"picker","id":2}"#],
        "the transferred name is probed through the checks child"
    );
    f.checks.say(Line::Reply(
        r#"{"t":"picker","id":2,"op":"save","ok":true,"review":1,"path":"/d/report.txt","collision":true}"#.into(),
    ));
    f.settle(|f| f.read(|picker| picker.accept_state().save_ready()));
    f.cx.simulate_keystrokes("right");
    f.cx.simulate_keystrokes("enter");
    f.cx.run_until_parked();
    assert_eq!(
        f.read(|picker| (picker.stem_requests, picker.answer_label().to_string())),
        (1, "Replace".into()),
        "Enter in Filename on a taken name reaches the Replace review"
    );
    assert_eq!(
        filename_state(&mut f),
        ("report.txt".into(), 0..6, true),
        "the review reselects the stem in Filename"
    );
    assert_eq!(
        f.checks.sent().len(),
        2,
        "a review answers nothing until Replace"
    );
    assert!(f.written().is_none());
}

#[gpui::test]
fn a_stale_or_unparsable_listing_line_lands_nothing(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    f.cx.run_until_parked();
    listed(&mut f, "/elsewhere", &[("x.txt", false)]);
    assert_eq!(
        f.read(|picker| picker.listing.state),
        "loading",
        "an answer for another folder is stale"
    );
    f.update(|picker, cx| picker.open_folder("/e", cx));
    listed(&mut f, "/d", &[("late.txt", false)]);
    assert_eq!(
        f.read(|picker| picker.listing.state),
        "loading",
        "the answer to an older request is stale"
    );
    f.listing.say(Line::Reply("{\"t\":\"rows\",".into()));
    f.cx.run_until_parked();
    assert!(
        f.read(|picker| picker.listing.failed),
        "an unparsable line fails, never reads as empty"
    );
}

#[gpui::test]
fn a_typed_file_is_peeked_located_and_answered(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    f.cx.run_until_parked();
    listed(&mut f, "/d", &[("a.txt", false), ("b.txt", false)]);
    let path = f.update(|picker, cx| picker.browse.path_view(cx));
    let asked = f.listing.sent().len();
    f.cx.update(|window, cx| path.update(cx, |view, cx| view.open(window, cx)));
    f.cx.run_until_parked();
    answer_jumps(&mut f, asked);
    let asked = f.listing.sent().len();
    f.cx.simulate_input("b.txt");
    f.cx.run_until_parked();
    answer_jumps(&mut f, asked);
    f.cx.simulate_keystrokes("enter");
    f.cx.run_until_parked();
    assert_eq!(
        f.listing.sent().last().map(String::as_str),
        Some(r#"{"c":"peek","path":"/d/b.txt","first":0,"hidden":false,"hiddenLast":false}"#)
    );
    f.listing.say(Line::Reply(r#"{"t":"peeked","path":"/d/b.txt","hidden":0,"hiddenLast":false,"first":0,"n":0,"failed":true,"mode":33188,"rows":[]}"#.into()));
    f.cx.run_until_parked();
    assert_eq!(
        f.listing.sent().last().map(String::as_str),
        Some(r#"{"c":"locate","path":"/d/b.txt"}"#)
    );
    f.listing.say(Line::Reply(
        r#"{"t":"located","directory":"/d","path":"/d/a.txt","index":0}"#.into(),
    ));
    f.cx.run_until_parked();
    assert!(
        f.checks.sent().is_empty(),
        "a located reply for another leaf is stale"
    );
    f.listing.say(Line::Reply(
        r#"{"t":"located","directory":"/d","path":"/d/b.txt","index":1}"#.into(),
    ));
    f.cx.run_until_parked();
    assert_eq!(f.read(|picker| picker.listing.cursor), 1);
    assert_eq!(
        f.checks.sent(),
        [
            r#"{"op":"mark","path":"/d/b.txt","directory":false,"multiple":false,"c":"picker","id":1}"#
        ],
        "the located row answers through the picker's own accept"
    );
}

#[gpui::test]
fn a_typed_folder_lists_and_a_missing_path_says_so(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    f.cx.run_until_parked();
    listed(&mut f, "/d", &[("sub", true)]);
    let path = f.update(|picker, cx| picker.browse.path_view(cx));
    for (typed, answer) in [
        (
            "gone",
            r#"{"t":"peeked","path":"/d/gone","hidden":0,"hiddenLast":false,"first":0,"n":0,"failed":true,"rows":[]}"#,
        ),
        (
            "sub",
            r#"{"t":"peeked","path":"/d/sub","hidden":0,"hiddenLast":false,"first":0,"n":0,"v":1,"w":true,"mode":16877,"rows":[]}"#,
        ),
    ] {
        // A refused folder reopens the bar with its text, so close it before the next attempt starts clean.
        f.cx.update(|_, cx| path.update(cx, |view, cx| view.close(cx)));
        f.cx.run_until_parked();
        let asked = f.listing.sent().len();
        f.cx.update(|window, cx| path.update(cx, |view, cx| view.open(window, cx)));
        f.cx.run_until_parked();
        answer_jumps(&mut f, asked);
        let asked = f.listing.sent().len();
        f.cx.simulate_input(typed);
        f.cx.run_until_parked();
        answer_jumps(&mut f, asked);
        f.cx.simulate_keystrokes("enter");
        f.cx.run_until_parked();
        f.listing.say(Line::Reply(answer.into()));
        f.cx.run_until_parked();
    }
    assert!(
        f.read(|picker| picker.message.as_ref().is_some_and(|message| message.error)),
        "a missing path is said loud"
    );
    assert_eq!(
        f.listing.sent().last().map(String::as_str),
        Some(
            r#"{"c":"list","path":"/d/sub","first":110,"pickerGlobs":[],"pickerMimes":[],"hidden":false,"pickerGen":2}"#
        ),
        "a peeked folder lists through the picker's own open, asking for the visible window"
    );
    assert_eq!(f.read(|picker| picker.listing.location.clone()), "/d/sub");
}

// EY-498: PathBar.js hands a typed smb:// line back untouched and the chooser peeks it as it peeks a path.
#[gpui::test]
fn ey498_a_typed_share_is_peeked_and_a_missing_one_says_so(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    f.cx.run_until_parked();
    listed(&mut f, "/d", &[("sub", true)]);
    let path = f.update(|picker, cx| picker.browse.path_view(cx));
    let asked = f.listing.sent().len();
    f.cx.update(|window, cx| path.update(cx, |view, cx| view.open(window, cx)));
    f.cx.run_until_parked();
    answer_jumps(&mut f, asked);
    let asked = f.listing.sent().len();
    f.cx.simulate_input("smb://127.0.0.1/missing-wave2");
    f.cx.run_until_parked();
    answer_jumps(&mut f, asked);
    f.cx.simulate_keystrokes("enter");
    f.cx.run_until_parked();
    assert_eq!(
        f.listing.sent().last().map(String::as_str),
        Some(
            r#"{"c":"peek","path":"smb://127.0.0.1/missing-wave2","first":0,"hidden":false,"hiddenLast":false}"#
        ),
        "the typed share is peeked, never dropped"
    );
    f.listing.say(Line::Reply(r#"{"t":"peeked","path":"smb://127.0.0.1/missing-wave2","hidden":0,"hiddenLast":false,"first":0,"n":0,"failed":true,"rows":[]}"#.into()));
    f.cx.run_until_parked();
    let message = f.read(|picker| picker.message.clone());
    assert!(
        message.as_ref().is_some_and(
            |message| message.error && message.text.contains("smb://127.0.0.1/missing-wave2")
        ),
        "a missing share is said loud: {:?}",
        message.map(|message| message.text)
    );
    assert_eq!(f.read(|picker| picker.listing.location.clone()), "/d");
}

#[test]
fn each_child_takes_only_its_own_role_lines() {
    use process::{admits, CHECKS, LISTING};
    for line in [
        r#"{"c":"list","path":"/d"}"#,
        r#"{"c":"peek","path":"/d"}"#,
        r#"{"c":"locate","path":"/d/a"}"#,
    ] {
        assert!(admits(LISTING, line) && !admits(CHECKS, line), "{line}");
    }
    let check = r#"{"op":"save","c":"picker","id":1}"#;
    assert!(admits(CHECKS, check) && !admits(LISTING, check));
    assert!(
        !admits(LISTING, r#"{"c":"trash","rows":[0]}"#),
        "the listing child never takes a write"
    );
}
