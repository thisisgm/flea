//! A keyboard or platform edit of the save name sends one probe, and the reply to it stays valid.
use super::{process::Line, tests::fixture::fixture};
use gpui::{EntityInputHandler, TestAppContext};

#[gpui::test]
fn i50_picker_filename_keyboard_probes_once_and_accepts_current_reply(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"mode":"save","name":"draft.txt","folder":"/d"}"#,
        None,
    );
    f.cx.run_until_parked();
    let field = f.read(|picker| picker.browse.filename.clone().unwrap());
    let before = f.checks.sent().len();
    f.cx.simulate_keystrokes("end backspace");
    f.cx.run_until_parked();
    assert_eq!(
        f.read(|picker| picker.listing.save_name.clone()),
        "draft.tx"
    );
    assert_eq!(f.checks.sent().len(), before + 1);
    let sent = f.checks.sent();
    let probe = sent.last().unwrap();
    let id = flea::json::field_usize(probe, "id").unwrap();
    assert!(probe.contains(r#""name":"draft.tx""#));
    f.checks.say(Line::Reply(format!(r#"{{"t":"picker","id":{id},"op":"save","ok":true,"review":{id},"path":"/d/draft.tx","collision":false}}"#)));
    f.settle(|f| f.read(|picker| picker.accept_state().save_ready()));
    assert_eq!(
        f.checks.sent().len(),
        before + 1,
        "the current reply remains valid"
    );
    f.cx.update(|window, cx| {
        field.update(cx, |field, cx| {
            field.replace_text_in_range(None, "t", window, cx);
        })
    });
    f.cx.run_until_parked();
    assert_eq!(
        f.read(|picker| picker.listing.save_name.clone()),
        "draft.txt"
    );
    assert_eq!(f.checks.sent().len(), before + 2);
    assert!(!f.read(|picker| picker.accept_state().save_ready()));
}

#[gpui::test]
fn i50_picker_path_candidate_enter_cannot_accept_or_navigate(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    f.cx.run_until_parked();
    f.listing
        .say(Line::Reply(r#"{"t":"listed","n":1,"path":"/d"}"#.into()));
    f.listing.say(Line::Reply(r#"{"t":"rows","start":0,"rows":[{"n":"notes.txt","d":false,"s":1,"m":0,"p":33188,"i":"text-x-generic"}],"listing":1}"#.into()));
    f.cx.run_until_parked();
    let focus = f.read(|picker| picker.focus.clone());
    f.cx.update(|window, cx| window.focus(&focus, cx));
    f.cx.simulate_keystrokes("ctrl-l");
    let path = f.update(|picker, cx| picker.browse.path_view(cx));
    f.cx.update(|window, cx| {
        path.update(cx, |path, cx| {
            assert!(path.holds_keyboard(window));
            path.replace_and_mark_text_in_range(None, "/d/\u{65e5}", None, window, cx);
            assert!(path.entry.marked.is_some());
        })
    });
    f.cx.run_until_parked();
    let before = f.listing.sent();
    f.cx.simulate_keystrokes("enter");
    f.cx.run_until_parked();
    assert_eq!(f.listing.sent(), before);
    assert!(f.checks.sent().is_empty());
    assert!(f.written().is_none());
    f.update(|picker, cx| picker.accept(cx));
    assert!(
        f.checks.sent().is_empty(),
        "retained accept respects path composition"
    );
    f.cx.update(|window, cx| {
        path.update(cx, |path, cx| {
            assert!(path.holds_keyboard(window));
            path.replace_text_in_range(None, "/d/\u{65e5}", window, cx);
        })
    });
    f.cx.simulate_keystrokes("enter");
    f.cx.run_until_parked();
    assert_eq!(f.listing.sent().len(), before.len() + 1);
    assert!(f
        .listing
        .sent()
        .last()
        .unwrap()
        .contains("\"path\":\"/d/\u{65e5}\""));
    assert!(f.checks.sent().is_empty());
}

#[gpui::test]
fn i50_picker_cancel_detaches_the_mounted_path(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    f.cx.run_until_parked();
    let path = f.update(|picker, cx| picker.browse.path_view(cx));
    f.cx.update(|window, cx| path.update(cx, |path, cx| path.open(window, cx)));
    f.cx.run_until_parked();
    f.cx.update(|window, cx| {
        path.update(cx, |path, cx| {
            path.replace_and_mark_text_in_range(None, "/d/\u{65e5}", None, window, cx);
            assert!(path.entry.marked.is_some());
        })
    });
    f.update(|picker, cx| picker.cancel(cx));
    f.settle(|f| f.written().is_some());
    assert_eq!(f.written().as_deref(), Some(r#"{"response":1}"#));
    let before = (f.listing.sent(), f.checks.sent());
    f.cx.update(|window, cx| {
        path.update(cx, |path, cx| {
            assert!(
                !path.editing(),
                "portal cancellation closes the path input session"
            );
            let text = path.entry.text.clone();
            path.replace_text_in_range(None, "late", window, cx);
            path.replace_and_mark_text_in_range(None, "late", None, window, cx);
            assert_eq!(path.entry.text, text);
            assert!(path.selected_text_range(false, window, cx).is_none());
            path.commit(cx);
        })
    });
    f.cx.run_until_parked();
    assert_eq!((f.listing.sent(), f.checks.sent()), before);
}

#[gpui::test]
fn i50_picker_cancel_refuses_queued_path_events(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    f.update(|picker, cx| {
        picker.path_out(
            crate::path::view::Out::Open {
                side: 0,
                path: "/d/sub".into(),
            },
            cx,
        )
    });
    f.update(|picker, cx| picker.cancel(cx));
    f.settle(|f| f.written().is_some());
    let before = f.listing.sent();
    f.listing.say(Line::Reply(r#"{"t":"peeked","path":"/d/sub","hidden":0,"hiddenLast":false,"first":0,"n":0,"v":1,"w":true,"mode":16877,"rows":[]}"#.into()));
    f.cx.run_until_parked();
    f.update(|picker, cx| {
        picker.path_out(
            crate::path::view::Out::Open {
                side: 0,
                path: "/late".into(),
            },
            cx,
        );
        picker.path_out(
            crate::path::view::Out::Send {
                side: 0,
                line: "late".into(),
            },
            cx,
        );
    });
    assert_eq!(
        f.listing.sent(),
        before,
        "a queued source event cannot outlive the portal"
    );
}
