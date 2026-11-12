//! Back escapes unconfirmed folders without losing the last folder the chooser visited.
use super::{
    process::Line,
    side_button_tests::{failed, landed, press},
    tests::fixture::{fixture, Fixture},
};
use gpui::{Modifiers, NavigationDirection, TestAppContext};

fn back(f: &mut Fixture, method: &str) {
    match method {
        "pointer" => press(f, NavigationDirection::Back),
        "keyboard" => {
            let focus = f.read(|p| p.focus.clone());
            f.cx.update(|w, cx| w.focus(&focus, cx));
            f.cx.simulate_keystrokes("shift-h->H");
            f.cx.run_until_parked();
        }
        "button" => {
            let at = f.cx.debug_bounds("picker-back").unwrap().center();
            f.cx.simulate_click(at, Modifiers::none());
            f.cx.run_until_parked();
        }
        _ => {
            f.update(|p, cx| p.go_back(cx));
        }
    }
}

#[gpui::test]
fn back_leaves_a_pending_open_folder(cx: &mut TestAppContext) {
    for method in ["owner", "pointer", "keyboard", "button"] {
        let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
        landed(&mut f, "/d");
        f.update(|p, cx| p.open_folder("/slow", cx));
        f.cx.run_until_parked();
        assert_eq!(
            f.read(|p| p.chrome.history.clone()),
            ["/d"],
            "departure records its source immediately"
        );
        back(&mut f, method);
        assert_eq!(f.read(|p| p.listing.location.clone()), "/d", "{method}");
    }
}

#[gpui::test]
fn back_after_failed_open_returns_to_the_folder_left(cx: &mut TestAppContext) {
    for method in ["owner", "pointer", "keyboard", "button"] {
        let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
        landed(&mut f, "/d");
        f.update(|p, cx| p.open_folder("/d/sub", cx));
        landed(&mut f, "/d/sub");
        let before = f.read(|p| (p.chrome.history.clone(), p.chrome.ahead.clone()));
        f.update(|p, cx| p.open_folder("/missing", cx));
        f.listing.say(Line::Reply(
            r#"{"t":"error","where":"scan","path":"/missing","msg":"missing"}"#.into(),
        ));
        f.cx.run_until_parked();
        back(&mut f, method);
        assert_eq!(f.read(|p| p.listing.location.clone()), "/d/sub", "{method}");
        assert_eq!(
            f.read(|p| (p.chrome.history.clone(), p.chrome.ahead.clone())),
            before
        );
    }
}

#[gpui::test]
fn back_after_failed_open_without_history_restores_the_source(cx: &mut TestAppContext) {
    for method in ["owner", "pointer", "keyboard", "button"] {
        let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
        landed(&mut f, "/d");
        f.update(|p, cx| p.open_folder("/missing/deep", cx));
        f.listing.say(Line::Reply(
            r#"{"t":"error","where":"scan","path":"/missing/deep","msg":"missing"}"#.into(),
        ));
        f.cx.run_until_parked();
        back(&mut f, method);
        assert_eq!(f.read(|p| p.listing.location.clone()), "/d", "{method}");
        assert!(f.read(|p| p.chrome.history.is_empty() && p.chrome.ahead.is_empty()));
    }
}

#[gpui::test]
fn abandoned_open_replies_do_not_replace_the_returned_folder(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    landed(&mut f, "/d");
    f.update(|p, cx| p.open_folder("/slow", cx));
    back(&mut f, "pointer");
    landed(&mut f, "/slow");
    assert_eq!(f.read(|p| p.listing.location.clone()), "/d");
    assert_eq!(f.read(|p| p.listing.state), "loading");
    assert_eq!(
        f.read(|p| p.listing.total),
        0,
        "late rows never replace the returned folder"
    );
    landed(&mut f, "/d");
    assert_eq!(f.read(|p| p.listing.state), "ready");
    assert!(f.read(|p| p.chrome.history.is_empty() && p.chrome.ahead.is_empty()));
}

#[gpui::test]
fn failed_back_recovers_the_source_then_allows_a_retry(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    landed(&mut f, "/d");
    f.update(|p, cx| p.open_folder("/d/sub", cx));
    landed(&mut f, "/d/sub");
    back(&mut f, "owner");
    f.listing.say(Line::Reply(
        r#"{"t":"error","where":"scan","path":"/d","msg":"refused"}"#.into(),
    ));
    f.cx.run_until_parked();
    back(&mut f, "owner");
    assert_eq!(f.read(|p| p.listing.location.clone()), "/d/sub");
    assert_eq!(f.read(|p| p.chrome.history.clone()), ["/d"]);
    assert!(f.read(|p| p.chrome.ahead.is_empty()));
    landed(&mut f, "/d/sub");
    back(&mut f, "owner");
    landed(&mut f, "/d");
    assert_eq!(f.read(|p| p.chrome.ahead.clone()), ["/d/sub"]);
}

#[gpui::test]
fn back_abandons_a_pending_path_route_and_ignores_its_late_peek(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    landed(&mut f, "/d");
    f.update(|p, cx| p.open_folder("/d/sub", cx));
    landed(&mut f, "/d/sub");
    f.update(|p, cx| p.open_folder("/d/sub/deep", cx));
    landed(&mut f, "/d/sub/deep");
    back(&mut f, "owner");
    landed(&mut f, "/d/sub");
    f.update(|p, _| p.browse.route.asked = "/pending".into());
    let before = f.listing.sent().len();
    press(&mut f, NavigationDirection::Forward);
    assert_eq!(f.listing.sent().len(), before);
    assert_eq!(f.read(|p| p.chrome.ahead.clone()), ["/d/sub/deep"]);
    back(&mut f, "pointer");
    assert_eq!(f.read(|p| p.listing.location.clone()), "/d");
    assert!(f.read(|p| p.browse.route.asked.is_empty()));
    f.listing.say(Line::Reply(
        r#"{"t":"peeked","path":"/pending","first":0,"mode":16877,"rows":[]}"#.into(),
    ));
    f.cx.run_until_parked();
    assert_eq!(f.read(|p| p.listing.location.clone()), "/d");
    landed(&mut f, "/d");
}

#[gpui::test]
fn returned_folder_lands_when_the_backend_silently_abandons_a_slow_scan(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    landed(&mut f, "/d");
    f.update(|p, cx| p.open_folder("/slow", cx));
    let old = f.update(|p, cx| p.browse.listing.read(cx).generation());
    back(&mut f, "owner");
    let current = f.update(|p, cx| p.browse.listing.read(cx).generation());
    f.listing.say(Line::Reply(format!(
        r#"{{"t":"listed","n":1,"path":"/d","pickerGen":{current}}}"#
    )));
    f.listing.say(Line::Reply(format!(r#"{{"t":"rows","start":0,"listing":3,"pickerGen":{current},"rows":[{{"n":"kept.txt","d":false,"s":1,"m":0,"p":33188,"i":"text-x-generic"}}]}}"#)));
    f.cx.run_until_parked();
    assert_eq!(
        f.read(|p| p.listing.state),
        "ready",
        "the abandoned scan need not send an answer"
    );
    assert_eq!(f.read(|p| p.listing.names.clone()), ["kept.txt"]);
    for (path, generation) in [("/slow", old), ("/d", old - 1)] {
        f.listing.say(Line::Reply(format!(
            r#"{{"t":"listed","n":0,"path":"{path}","pickerGen":{generation}}}"#
        )));
        f.listing.say(Line::Reply(format!(r#"{{"t":"error","where":"scan","path":"{path}","msg":"stale","pickerGen":{generation}}}"#)));
        f.listing.say(Line::Reply(format!(
            r#"{{"t":"rows","start":0,"listing":1,"rows":[],"pickerGen":{generation}}}"#
        )));
    }
    f.cx.run_until_parked();
    assert_eq!(f.read(|p| p.listing.state), "ready");
    assert_eq!(
        f.read(|p| p.listing.names.clone()),
        ["kept.txt"],
        "old path and same-path replies are both refused"
    );
}

#[gpui::test]
fn failed_continued_back_keeps_every_confirmed_folder(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    landed(&mut f, "/d");
    f.update(|p, cx| p.open_folder("/d/sub", cx));
    landed(&mut f, "/d/sub");
    f.update(|p, cx| p.open_folder("/d/sub/deep", cx));
    landed(&mut f, "/d/sub/deep");
    let before = f.read(|p| (p.chrome.history.clone(), p.chrome.ahead.clone()));
    back(&mut f, "owner");
    back(&mut f, "owner");
    assert_eq!(f.read(|p| p.listing.location.clone()), "/d");
    failed(&mut f, "/d");
    back(&mut f, "owner");
    assert_eq!(f.read(|p| p.listing.location.clone()), "/d/sub/deep");
    assert_eq!(
        f.read(|p| (p.chrome.history.clone(), p.chrome.ahead.clone())),
        before,
        "a failed run of Back presses restores every folder it passed"
    );
    landed(&mut f, "/d/sub/deep");
    back(&mut f, "owner");
    assert_eq!(f.read(|p| p.listing.location.clone()), "/d/sub");
}

#[gpui::test]
fn late_rows_of_an_older_generation_never_replace_the_current_rows(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    landed(&mut f, "/d");
    let old = f.update(|p, cx| p.browse.listing.read(cx).generation());
    f.update(|p, cx| p.open_folder("/d", cx));
    landed(&mut f, "/d");
    assert_eq!(f.read(|p| p.listing.names.clone()), ["sub"]);
    f.listing.say(Line::Reply(format!(r#"{{"t":"rows","start":0,"listing":1,"rows":[{{"n":"late.txt","d":false,"s":1,"m":0,"p":33188,"i":"text-x-generic"}}],"pickerGen":{old}}}"#)));
    f.cx.run_until_parked();
    assert_eq!(
        f.read(|p| p.listing.names.clone()),
        ["sub"],
        "a reply tagged with an older pickerGen is dropped even after the current one lands"
    );
}
