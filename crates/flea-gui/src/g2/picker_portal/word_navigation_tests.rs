//! Mounted Save As, picker path and New Folder fields retain word chords and never navigate the chooser.
use super::{process::Line, tests::fixture::fixture};
use crate::path::entry::navigation_cases::{check, snapshot, COMPOSING};
use gpui::{EntityInputHandler, Modifiers, TestAppContext};

#[gpui::test]
fn fx466_picker_filename_word_navigation(cx: &mut TestAppContext) {
    name(cx, false);
}

#[gpui::test]
fn fx466_picker_new_folder_word_navigation(cx: &mut TestAppContext) {
    name(cx, true);
}

fn name(cx: &mut TestAppContext, folder: bool) {
    let mut f = fixture(
        cx,
        r#"{"mode":"save","folder":"/d","name":"draft.txt"}"#,
        None,
    );
    f.listing
        .say(Line::Reply(r#"{"t":"listed","n":1,"path":"/d"}"#.into()));
    f.listing.say(Line::Reply(r#"{"t":"rows","start":0,"listing":1,"rows":[{"n":"sub","d":true,"s":0,"m":0,"p":16877,"i":"folder"}]}"#.into()));
    f.cx.run_until_parked();
    if folder {
        let at = f.cx.debug_bounds("picker-new-folder").unwrap().center();
        f.cx.simulate_click(at, Modifiers::none());
        f.cx.run_until_parked();
    }
    let field = f
        .read(|p| {
            if folder {
                p.chrome.folder.clone()
            } else {
                p.browse.filename.clone()
            }
        })
        .unwrap();
    f.cx.update(|window, cx| {
        let focus = field.read(cx).focus.clone();
        window.focus(&focus, cx);
    });
    check(|text, keys| {
        f.cx.update(|_, cx| {
            field.update(cx, |field, cx| {
                field.entry.begin(text);
                field.entry.move_to(0);
                cx.notify();
            })
        });
        f.cx.run_until_parked();
        let before = (f.listing.sent(), f.checks.sent());
        f.cx.simulate_keystrokes(keys);
        f.cx.run_until_parked();
        assert_eq!(
            (f.listing.sent(), f.checks.sent()),
            before,
            "field navigation sends no requests"
        );
        assert_eq!(f.read(|p| p.listing.location.clone()), "/d");
        f.cx.update(|_, cx| snapshot(&field.read(cx).entry))
    });
    f.cx.update(|window, cx| {
        field.update(cx, |field, cx| {
            field.replace_and_mark_text_in_range(None, "にほん", None, window, cx)
        })
    });
    let before = f.cx.update(|_, cx| snapshot(&field.read(cx).entry));
    assert!(
        before.4.is_some(),
        "the composition must be live: {before:?}"
    );
    let sent = (f.listing.sent(), f.checks.sent());
    for keys in COMPOSING {
        f.cx.simulate_keystrokes(keys);
        f.cx.run_until_parked();
        let now = (f.listing.sent(), f.checks.sent());
        assert_eq!(now, sent, "{keys}: composition sends no requests");
        assert_eq!(f.read(|p| p.listing.location.clone()), "/d");
        let after = f.cx.update(|_, cx| snapshot(&field.read(cx).entry));
        assert_eq!(after, before, "{keys}");
    }
}

#[gpui::test]
fn fx466_picker_path_word_navigation(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    f.cx.run_until_parked();
    let view = f.update(|p, cx| p.browse.path_view(cx));
    f.cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open(window, cx);
        })
    });
    check(|text, keys| {
        f.cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.entry.begin(text);
                view.entry.move_to(0);
                cx.notify();
            })
        });
        f.cx.run_until_parked();
        let before = (f.listing.sent(), f.checks.sent());
        f.cx.simulate_keystrokes(keys);
        f.cx.run_until_parked();
        assert_eq!((f.listing.sent(), f.checks.sent()), before);
        assert_eq!(f.read(|p| p.listing.location.clone()), "/d");
        f.cx.update(|_, cx| snapshot(&view.read(cx).entry))
    });
    f.cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.replace_and_mark_text_in_range(None, "にほん", None, window, cx)
        })
    });
    let before = f.cx.update(|_, cx| snapshot(&view.read(cx).entry));
    assert!(
        before.4.is_some(),
        "the composition must be live: {before:?}"
    );
    let sent = (f.listing.sent(), f.checks.sent());
    for keys in COMPOSING {
        f.cx.simulate_keystrokes(keys);
        f.cx.run_until_parked();
        let now = (f.listing.sent(), f.checks.sent());
        assert_eq!(now, sent, "{keys}: composition sends no requests");
        assert_eq!(f.read(|p| p.listing.location.clone()), "/d");
        let after = f.cx.update(|_, cx| snapshot(&view.read(cx).entry));
        assert_eq!(after, before, "{keys}");
    }
}
