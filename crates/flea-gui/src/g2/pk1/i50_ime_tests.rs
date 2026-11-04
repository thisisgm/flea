//! Native chooser text cannot activate the chooser while it is still composing.
use super::{
    filename::Filename,
    process::Line,
    tests::fixture::{fixture, Fixture},
    Picker,
};
use flea::ui_logic_pk1::CLOSE_LINE;
use gpui::{Context, EntityInputHandler, TestAppContext, Window};

fn filename<R>(
    f: &mut Fixture,
    apply: impl FnOnce(&mut Filename, &mut Window, &mut Context<Filename>) -> R,
) -> R {
    let field = f.read(|picker| picker.browse.filename.clone().unwrap());
    let result =
        f.cx.update(|window, cx| field.update(cx, |field, cx| apply(field, window, cx)));
    f.cx.run_until_parked();
    result
}

fn filter<R>(
    f: &mut Fixture,
    apply: impl FnOnce(&mut Picker, &mut Window, &mut Context<Picker>) -> R,
) -> R {
    let picker = f.picker.clone();
    let result =
        f.cx.update(|window, cx| picker.update(cx, |picker, cx| apply(picker, window, cx)));
    f.cx.run_until_parked();
    result
}

#[gpui::test]
fn i50_picker_filename_compose(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"mode":"save","name":"draft.txt","folder":"/d"}"#,
        None,
    );
    f.cx.run_until_parked();
    filename(&mut f, |field, window, cx| {
        assert!(
            field.entry.editing,
            "the mounted field opens a platform session"
        );
        field.replace_and_mark_text_in_range(None, "\u{1d11e}\u{65e5}", Some(0..2), window, cx);
        assert_eq!(field.marked_text_range(window, cx), Some(0..3));
        assert_eq!(
            field.selected_text_range(false, window, cx).unwrap().range,
            0..2
        );
        let mut adjusted = None;
        assert_eq!(
            field
                .text_for_range(1..2, &mut adjusted, window, cx)
                .as_deref(),
            Some("\u{1d11e}")
        );
        assert_eq!(adjusted, Some(0..2));
    });
    let before = f.checks.sent().len();
    f.cx.simulate_keystrokes("enter");
    assert_eq!(
        f.checks.sent().len(),
        before,
        "candidate Enter cannot submit"
    );
    assert!(f.written().is_none());
    filename(&mut f, |field, window, cx| {
        let band = field
            .bounds_for_range(0..2, Default::default(), window, cx)
            .unwrap();
        assert!(band.size.width > gpui::px(0.) && band.size.height > gpui::px(0.));
        field.replace_text_in_range(None, "\u{5831}\u{544a}", window, cx);
        assert_eq!(field.marked_text_range(window, cx), None);
        let old = field.text().to_owned();
        field.replace_text_in_range(None, "bad\nname", window, cx);
        assert_eq!(field.text(), old, "control characters are refused whole");
    });
    assert_eq!(
        f.read(|picker| picker.listing.save_name.clone()),
        "\u{5831}\u{544a}.txt"
    );
    let probe = f.checks.sent().last().unwrap().clone();
    let id = flea::json::field_usize(&probe, "id").unwrap();
    let path = "/d/\u{5831}\u{544a}.txt";
    f.checks.say(Line::Reply(format!(r#"{{"t":"picker","id":{id},"op":"save","ok":true,"review":{id},"path":"{path}","collision":false}}"#)));
    f.cx.run_until_parked();
    assert!(
        !f.read(|picker| picker.accept_state().save_ready()),
        "an earlier preedit probe cannot validate the committed name"
    );
    let current = f.checks.sent().last().unwrap().clone();
    let current_id = flea::json::field_usize(&current, "id").unwrap();
    assert_ne!(current_id, id);
    f.checks.say(Line::Reply(format!(r#"{{"t":"picker","id":{current_id},"op":"save","ok":true,"review":{current_id},"path":"{path}","collision":false}}"#)));
    f.settle(|f| f.read(|picker| picker.accept_state().save_ready()));
    let before = f.checks.sent().len();
    f.cx.simulate_keystrokes("enter");
    assert_eq!(f.checks.sent().len(), before + 1);
    assert!(f.checks.sent().last().unwrap().contains(r#""op":"review""#));
}

#[gpui::test]
fn i50_picker_filter_compose(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    f.cx.run_until_parked();
    f.listing.say(Line::Reply(
        r#"{"t":"listed","n":1,"read":0.000,"sort":0.000,"v":1,"w":true,"path":"/d"}"#.into(),
    ));
    f.listing.say(Line::Reply(r#"{"t":"rows","start":0,"rows":[{"n":"notes.txt","d":false,"s":1,"m":0,"p":33188,"i":"text-x-generic"}],"listing":1}"#.into()));
    f.cx.run_until_parked();
    let focus = f.read(|picker| picker.focus.clone());
    f.cx.update(|window, cx| window.focus(&focus, cx));
    f.cx.simulate_keystrokes("/");
    assert!(
        f.read(|picker| picker.find.owner.typing()),
        "the real slash dispatch must open the ready chooser filter"
    );
    filter(&mut f, |picker, window, cx| {
        picker.replace_and_mark_text_in_range(None, "\u{1d11e}n", Some(2..3), window, cx);
        assert_eq!(picker.marked_text_range(window, cx), Some(0..3));
        assert_eq!(
            picker.selected_text_range(false, window, cx).unwrap().range,
            2..3
        );
    });
    let before = f.listing.sent().len();
    f.cx.simulate_keystrokes("enter down");
    assert!(f.read(|picker| picker.find.owner.typing()));
    assert_eq!(
        f.listing.sent().len(),
        before,
        "preedit does not narrow or navigate"
    );
    assert!(f.checks.sent().is_empty());
    filter(&mut f, |picker, window, cx| {
        let band = picker
            .bounds_for_range(0..2, Default::default(), window, cx)
            .unwrap();
        assert!(band.size.width > gpui::px(0.));
        picker.replace_text_in_range(None, "no", window, cx);
    });
    assert_eq!(f.read(|picker| picker.find.owner.query().to_owned()), "no");
    let id = f.read(|picker| picker.find.owner.asked().unwrap().id);
    f.listing.say(Line::Reply(format!(
        r#"{{"t":"filtered","id":{id},"query":"no","n":1,"rows":[0],"ms":0.010,"listing":1}}"#
    )));
    f.cx.run_until_parked();
    f.cx.simulate_keystrokes("enter");
    assert_eq!(
        f.checks.sent().len(),
        1,
        "Enter after composition activates exactly once"
    );
    assert!(f.checks.sent()[0].contains(r#""path":"/d/notes.txt""#));
}

#[gpui::test]
fn i50_picker_cancel_no_accept(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"mode":"save","name":"draft.txt","folder":"/d"}"#,
        None,
    );
    f.cx.run_until_parked();
    filename(&mut f, |field, window, cx| {
        field.replace_and_mark_text_in_range(None, "\u{5831}", None, window, cx)
    });
    f.cx.simulate_keystrokes("escape");
    assert!(
        f.written().is_none(),
        "composition Escape cannot cancel the portal"
    );
    filename(&mut f, |field, window, cx| {
        field.replace_and_mark_text_in_range(None, "\u{544a}", None, window, cx)
    });
    f.update(|picker, cx| picker.cancel(cx));
    f.settle(|f| f.written().is_some());
    assert_eq!(f.written().as_deref(), Some(r#"{"response":1}"#));
    let before = f.checks.sent();
    filename(&mut f, |field, window, cx| {
        let text = field.text().to_owned();
        field.replace_text_in_range(None, "late", window, cx);
        field.replace_and_mark_text_in_range(None, "late", None, window, cx);
        assert_eq!(field.text(), text);
        assert!(field.selected_text_range(false, window, cx).is_none());
    });
    assert_eq!(f.checks.sent(), before);
    assert_eq!(f.checks.sent().last().map(String::as_str), Some(CLOSE_LINE));
    assert!(!f
        .checks
        .sent()
        .iter()
        .any(|line| line.contains(r#""op":"review""#)));
}

#[gpui::test]
fn i50_picker_filter_tab_commits_preedit_without_accept(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    f.cx.run_until_parked();
    f.listing
        .say(Line::Reply(r#"{"t":"listed","n":1,"path":"/d"}"#.into()));
    f.listing.say(Line::Reply(r#"{"t":"rows","start":0,"rows":[{"n":"notes.txt","d":false,"s":1,"m":0,"p":33188,"i":"text-x-generic"}],"listing":1}"#.into()));
    f.cx.run_until_parked();
    let focus = f.read(|picker| picker.focus.clone());
    f.cx.update(|window, cx| window.focus(&focus, cx));
    f.cx.simulate_keystrokes("/");
    filter(&mut f, |picker, window, cx| {
        picker.replace_and_mark_text_in_range(None, "\u{7b46}\u{8a18}", None, window, cx)
    });
    f.cx.simulate_keystrokes("tab");
    assert_eq!(
        f.read(|picker| picker.find.owner.query().to_owned()),
        "\u{7b46}\u{8a18}"
    );
    assert!(f.read(|picker| picker.find.owner.typing()));
    assert!(f.checks.sent().is_empty());
}

#[gpui::test]
fn i50_picker_filter_cancel_detaches_composition(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    f.cx.run_until_parked();
    f.listing
        .say(Line::Reply(r#"{"t":"listed","n":1,"path":"/d"}"#.into()));
    f.listing.say(Line::Reply(r#"{"t":"rows","start":0,"rows":[{"n":"notes.txt","d":false,"s":1,"m":0,"p":33188,"i":"text-x-generic"}],"listing":1}"#.into()));
    f.cx.run_until_parked();
    let focus = f.read(|picker| picker.focus.clone());
    f.cx.update(|window, cx| window.focus(&focus, cx));
    f.cx.simulate_keystrokes("/");
    filter(&mut f, |picker, window, cx| {
        picker.replace_and_mark_text_in_range(None, "\u{7b46}", None, window, cx);
        picker.accept(cx);
        assert!(picker.find.owner.typing());
    });
    assert!(f.checks.sent().is_empty());
    f.update(|picker, cx| picker.cancel(cx));
    f.settle(|f| f.written().is_some());
    assert_eq!(f.written().as_deref(), Some(r#"{"response":1}"#));
    let before = f.listing.sent();
    filter(&mut f, |picker, window, cx| {
        let text = picker.find.entry.text.clone();
        picker.replace_text_in_range(None, "late", window, cx);
        picker.replace_and_mark_text_in_range(None, "late", None, window, cx);
        assert_eq!(picker.find.entry.text, text);
        assert!(picker.selected_text_range(false, window, cx).is_none());
    });
    assert_eq!(f.listing.sent(), before);
    assert_eq!(f.checks.sent(), [CLOSE_LINE]);
}
