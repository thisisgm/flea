// The save answer: probe the name, ask once on a taken name, and only Replace overwrites.
use super::*;

fn named(f: &mut Fixture, name: &str) {
    f.update(|picker, cx| {
        picker.listing.location = "/d".into();
        picker.listing.save_name = name.into();
        picker.invalidate_save(cx);
    });
}

fn probed(f: &mut Fixture, id: u64, collision: bool) {
    f.checks.say(Line::Reply(format!(
        r#"{{"t":"picker","id":{id},"op":"save","ok":true,"review":{id},"path":"/d/a.txt","collision":{collision}}}"#
    )));
    f.settle(|f| f.read(|picker| picker.accept_state().save_ready()));
}

#[gpui::test]
fn a_free_name_is_probed_reviewed_and_answered_with_its_path(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"mode":"save","name":"a.txt","folder":"/d"}"#, None);
    named(&mut f, "a.txt");
    assert_eq!(
        f.checks.sent(),
        [r#"{"op":"save","folder":"/d","name":"a.txt","c":"picker","id":1}"#]
    );
    assert!(
        !f.read(Picker::can_accept),
        "no answer before the probe returns"
    );
    probed(&mut f, 1, false);
    f.update(|picker, cx| picker.accept(cx));
    assert_eq!(
        f.checks.sent()[1],
        r#"{"op":"review","review":1,"c":"picker","id":2}"#
    );
    f.checks.say(Line::Reply(
        r#"{"t":"picker","id":2,"op":"review","ok":true,"path":"/d/a.txt","collision":false}"#
            .into(),
    ));
    f.settle(|f| f.written().is_some());
    assert_eq!(
        f.written().as_deref(),
        Some(r#"{"response":0,"uris":["file:///d/a.txt"],"filter":-1}"#)
    );
}

#[gpui::test]
fn a_taken_name_asks_once_and_only_replace_overwrites(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"mode":"save"}"#, None);
    named(&mut f, "a.txt");
    probed(&mut f, 1, true);
    f.update(|picker, cx| picker.accept(cx));
    assert_eq!(
        f.read(|picker| (picker.stem_requests, picker.answer_label().to_string())),
        (1, "Replace".into())
    );
    f.update(|picker, cx| picker.accept(cx));
    assert_eq!(
        f.checks.sent().len(),
        1,
        "Return on a standing review never submits"
    );
    f.update(|picker, cx| picker.replace(cx));
    assert_eq!(
        f.checks.sent()[1],
        r#"{"op":"review","review":1,"c":"picker","id":2}"#
    );
    f.checks.say(Line::Reply(
        r#"{"t":"picker","id":2,"op":"review","ok":true,"path":"/d/a.txt","collision":true}"#
            .into(),
    ));
    f.settle(|f| f.written().is_some());
    assert_eq!(
        f.written().as_deref(),
        Some(r#"{"response":0,"uris":["file:///d/a.txt"],"filter":-1}"#)
    );
}

#[gpui::test]
fn a_refused_review_says_why_and_probes_again(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"mode":"save"}"#, None);
    named(&mut f, "a.txt");
    probed(&mut f, 1, false);
    f.update(|picker, cx| picker.accept(cx));
    f.checks.say(Line::Reply(
        r#"{"t":"picker","id":2,"op":"review","ok":false,"error":""}"#.into(),
    ));
    f.settle(|f| f.checks.sent().len() == 3);
    let message = f.read(|picker| picker.message.clone().unwrap());
    assert_eq!(
        (message.text.as_ref(), message.error),
        ("The save location changed; review it again.", true)
    );
    assert_eq!(
        f.checks.sent()[2],
        r#"{"op":"save","folder":"/d","name":"a.txt","c":"picker","id":3}"#
    );
    assert!(f.written().is_none());
}

#[gpui::test]
fn a_probe_for_an_old_name_is_asked_again_for_the_new_one(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"mode":"save"}"#, None);
    named(&mut f, "a.txt");
    // The name changes while the first probe is out; the probe in flight blocks a second one.
    f.update(|picker, cx| {
        picker.listing.save_name = "b.txt".into();
        picker.invalidate_save(cx);
    });
    assert_eq!(f.checks.sent().len(), 1);
    f.checks.say(Line::Reply(r#"{"t":"picker","id":1,"op":"save","ok":true,"review":1,"path":"/d/a.txt","collision":false}"#.into()));
    f.settle(|f| f.checks.sent().len() == 2);
    assert_eq!(
        f.checks.sent()[1],
        r#"{"op":"save","folder":"/d","name":"b.txt","c":"picker","id":2}"#
    );
    assert!(
        !f.read(|picker| picker.accept_state().save_ready()),
        "a stale probe never makes a name ready"
    );
}

#[gpui::test]
fn a_save_name_is_refused_before_any_probe(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"mode":"save","name":"../../pwn"}"#, None);
    assert_eq!(
        f.read(|picker| picker.listing.save_name.clone()),
        "",
        "a traversal never seeds the field"
    );
    f.update(|picker, cx| picker.accept(cx));
    assert_eq!(
        f.read(|picker| picker.message.clone().unwrap().text.to_string()),
        "Name the file before saving it"
    );
    named(&mut f, "../x");
    f.update(|picker, cx| picker.accept(cx));
    assert_eq!(
        f.read(|picker| picker.message.clone().unwrap().text.to_string()),
        "a name cannot be empty, . or .., or contain /"
    );
    assert!(f.checks.sent().is_empty());
}

#[gpui::test]
fn a_double_click_marks_then_sends_and_refuses_while_checking(cx: &mut TestAppContext) {
    let mut f = fixture(cx, "{}", None);
    let action = f.update(|picker, cx| picker.double_activate(Some(false), "/d/a", "/d/a", cx));
    assert_eq!(action, logic::DoubleAction::MarkAccept);
    assert_eq!(
        f.checks.sent(),
        [r#"{"op":"mark","path":"/d/a","directory":false,"multiple":false,"c":"picker","id":1}"#]
    );
    f.update(|picker, cx| picker.double_activate(Some(false), "/d/a", "/d/a", cx));
    assert_eq!(
        f.read(|picker| picker.message.clone().unwrap().text.to_string()),
        "Selection is still being checked."
    );
    assert_eq!(
        f.update(|picker, cx| picker.double_activate(Some(true), "/d/s", "/d/s", cx)),
        logic::DoubleAction::Open
    );
    assert_eq!(f.checks.sent().len(), 1);
}

// picker-open and picker-save: the caller's title, Cancel then the default, and the hint in keys.toml spelling.
#[gpui::test]
fn the_boards_strings_draw_for_open_and_save(cx: &mut TestAppContext) {
    for (request, title, answer, hints) in [
        (
            r#"{"title":"Open File"}"#,
            "Open File",
            "Open",
            "enter open · esc cancel",
        ),
        (
            r#"{"title":"Save File","mode":"save","name":"sample-download.zip"}"#,
            "Save File",
            "Save",
            "enter save · esc cancel",
        ),
    ] {
        let mut f = fixture(cx, request, None);
        let drawn = f.read(|picker| {
            (
                picker.labels.title.clone(),
                picker.labels.cancel.clone(),
                picker.answer_label(),
                picker.hints(),
                picker.labels.requester.clone(),
            )
        });
        assert_eq!(drawn.0.as_ref(), title);
        assert_eq!(drawn.1.as_ref(), "Cancel");
        assert_eq!(drawn.2.as_ref(), answer);
        assert_eq!(drawn.3.as_ref(), hints);
        assert!(drawn.4.is_none(), "no portal identity, no requester");
        f.update(|picker, cx| picker.cancel(cx));
        f.settle(|f| f.quits() == (1, 1));
    }
}

// Review g2-pk1-r1-2: in the collision review Replace is the destructive answer, never the accent default.
#[gpui::test]
fn replace_is_never_drawn_as_the_default(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"mode":"save"}"#, None);
    named(&mut f, "a.txt");
    probed(&mut f, 1, true);
    assert!(
        f.read(Picker::answer_is_default),
        "Save rests as the default"
    );
    f.update(|picker, cx| picker.accept(cx));
    assert_eq!(f.read(Picker::answer_label).as_ref(), "Replace");
    assert!(
        !f.read(Picker::answer_is_default),
        "Replace carries no accent frame or wash"
    );
}

// Review g2-pk1-r1-7: paint reads the counted label built once per count and locale, never re-translated.
#[gpui::test]
fn the_counted_label_is_built_once_per_count(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"multiple":true}"#, None);
    f.update(|picker, _| {
        picker.marks = logic::Mark::reviewed(&[], &[("/a".into(), 1), ("/b".into(), 1)])
    });
    let (first, second, built) = f.read(|picker| {
        let labels = (picker.answer_label(), picker.answer_label());
        (labels.0, labels.1, picker.labels.built.get())
    });
    assert_eq!((first.as_ref(), second.as_ref()), ("Open 2", "Open 2"));
    assert_eq!(built, 1, "a second paint translated the label again");
    f.update(|picker, _| picker.marks.truncate(1));
    assert_eq!(f.read(Picker::answer_label).as_ref(), "Open");
}

// Review g2-pk1-r2-3: a child whose lines end without Stopped (its reader never started) still counts as stopped.
#[gpui::test]
fn a_child_whose_lines_close_without_stopped_still_lets_the_window_exit(cx: &mut TestAppContext) {
    let mut f = fixture(cx, "{}", None);
    f.update(|picker, cx| picker.cancel(cx));
    f.settle(|f| f.quits() == (1, 1));
    f.checks.answer.close();
    f.listing.say(Line::Stopped);
    f.cx.run_until_parked();
    assert_eq!(
        f.exits.get(),
        1,
        "the answered window waited on a closed channel"
    );
}
