//! Preview parity through the mounted Root, real keys and live IPC readers.
use super::{
    preview_tests::{answer, close, code_reply, opened, press, rows, sent, token},
    test_shell::{land, set_cursor},
};
use crate::window::u07_tests::read;
use flea::jsondoc::{self, Json};
use gpui::TestAppContext;

#[gpui::test]
fn pb_preview_text_state_tracks_the_live_code_and_wrap_key(cx: &mut TestAppContext) {
    let (shell, cx, mut requests) = opened(cx);
    land(&shell, cx, "/files", rows(), 1);
    set_cursor(&shell, cx, 1);
    press(cx, "space");
    let ask = sent(&mut requests, "code");
    answer(
        &shell,
        cx,
        code_reply(token(&ask[0]), &["first", "second"], 2),
    );
    let value = jsondoc::parse(&read(&shell, cx, "previewTextState", &["false"])).unwrap();
    assert_eq!(
        value.get("path").and_then(Json::as_str),
        Some("/files/notes.txt")
    );
    assert_eq!(value.get("ready").and_then(Json::as_bool), Some(true));
    assert_eq!(value.get("wrap").and_then(Json::as_bool), Some(false));
    assert_eq!(value.get("total").and_then(Json::as_f64), Some(2.));
    press(cx, "w");
    let value = jsondoc::parse(&read(&shell, cx, "previewTextState", &["false"])).unwrap();
    assert_eq!(value.get("wrap").and_then(Json::as_bool), Some(true));
    press(cx, "escape");
    assert_eq!(read(&shell, cx, "previewTextState", &["false"]), "{}");
    close(&shell, cx);
}

/// EY-503: Preview.qml binds the code view's wrap to ViewState.previewWrap, so a saved preview.wrap opens Quick Look wrapped.
#[gpui::test]
fn fixpreview_ey503_quick_look_opens_with_the_saved_wrap(cx: &mut TestAppContext) {
    let (shell, cx, mut requests) = opened(cx);
    shell.update(cx, |shell, _| {
        let mut data = flea::uischema::defaults();
        if let Json::Obj(fields) = &mut data {
            for (key, value) in fields.iter_mut() {
                if let (true, Json::Obj(preview)) = (key == "preview", value) {
                    preview.retain(|(leaf, _)| leaf != "wrap");
                    preview.push(("wrap".into(), Json::Bool(true)));
                }
            }
        }
        shell.launch.settings.data = data;
    });
    land(&shell, cx, "/files", rows(), 1);
    set_cursor(&shell, cx, 1);
    press(cx, "space");
    let ask = sent(&mut requests, "code");
    answer(
        &shell,
        cx,
        code_reply(token(&ask[0]), &["first", "second"], 2),
    );
    let value = jsondoc::parse(&read(&shell, cx, "previewTextState", &["false"])).unwrap();
    assert_eq!(value.get("ready").and_then(Json::as_bool), Some(true));
    assert_eq!(
        value.get("wrap").and_then(Json::as_bool),
        Some(true),
        "the saved preview.wrap applies when Quick Look opens"
    );
    press(cx, "escape");
    close(&shell, cx);
}

/// EY-528: LoadingState.qml's hold, then the kind's mark over "reading", while a Quick Look code reply is out.
#[gpui::test]
fn fixpreview_ey528_quick_look_draws_the_waiting_mark_after_its_hold(cx: &mut TestAppContext) {
    let (shell, cx, mut requests) = opened(cx);
    land(&shell, cx, "/files", rows(), 1);
    set_cursor(&shell, cx, 1);
    press(cx, "space");
    let ask = sent(&mut requests, "code");
    let step = |cx: &mut gpui::VisualTestContext, ms: u64| {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(ms));
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
    };
    let view = shell.read_with(cx, |shell, _| {
        shell.quick_look.as_ref().unwrap().view.clone()
    });
    let notified = std::rc::Rc::new(std::cell::Cell::new(0));
    let _watch = cx.update(|_, cx| {
        let notified = notified.clone();
        cx.observe(&view, move |_, _| notified.set(notified.get() + 1))
    });
    step(cx, 100);
    assert!(
        cx.debug_bounds("pv1-waiting").is_none(),
        "nothing flashes inside the hold"
    );
    // One millisecond either side of the hold's end, counted from the 100 ms step above.
    step(cx, crate::pane::swap::HOLD_MS - 100 - 1);
    let before = notified.get();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(2));
    cx.run_until_parked();
    assert!(
        notified.get() > before,
        "the hold's own end repaints an idle card"
    );
    step(cx, 1050);
    let mark = cx
        .debug_bounds("pv1-waiting")
        .expect("past HOLD_MS the waiting card draws its mark");
    let card = cx.debug_bounds("pv1-card").expect("the card is shown");
    assert!(card.contains(&mark.center()), "the mark sits in the card");
    answer(
        &shell,
        cx,
        code_reply(token(&ask[0]), &["first", "second"], 2),
    );
    step(cx, 16);
    assert!(
        cx.debug_bounds("pv1-waiting").is_none(),
        "the answer replaces the mark"
    );
    press(cx, "escape");
    close(&shell, cx);
}

/// EY-502: a Compare whose diff is out reads loading, draws its bar alone and then the shared waiting mark, never an empty refusal.
#[gpui::test]
fn fixpreview_ey502_a_held_compare_reads_loading_and_draws_the_waiting_mark(
    cx: &mut TestAppContext,
) {
    let (shell, cx, mut requests) = opened(cx);
    super::compare_tests::pair(&shell, cx);
    let opened =
        cx.update(|window, cx| shell.update(cx, |shell, cx| shell.open_compare(0, window, cx)));
    assert!(opened, "two selected regular files open Compare");
    let asked = sent(&mut requests, "compare");
    assert_eq!(asked.len(), 1, "one diff asked per open");
    let step = |cx: &mut gpui::VisualTestContext, ms: u64| {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(ms));
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
    };
    step(cx, 100);
    assert_eq!(read(&shell, cx, "previewKind", &[]), "compare");
    assert_eq!(
        read(&shell, cx, "previewState", &[]),
        "loading",
        "revealed with the diff out"
    );
    assert!(
        cx.debug_bounds("pv1-waiting").is_none(),
        "nothing flashes inside the hold"
    );
    assert!(
        cx.debug_bounds("pv1-recipe").is_none(),
        "the bar draws alone, never an empty refusal"
    );
    step(cx, 1100);
    assert_eq!(
        read(&shell, cx, "previewState", &[]),
        "loading",
        "still out past the hold"
    );
    assert!(
        cx.debug_bounds("pv1-waiting").is_some(),
        "past HOLD_MS the Compare card draws the waiting mark"
    );
    assert!(
        cx.debug_bounds("pv1-recipe").is_none(),
        "still no empty refusal past the hold"
    );
    let line = format!(
        r#"{{"t":"compare","token":{},"state":"identical","patch":""}}"#,
        token(&asked[0])
    );
    answer(&shell, cx, line);
    assert_eq!(
        read(&shell, cx, "previewState", &[]),
        "The two files are identical."
    );
    assert!(
        cx.debug_bounds("pv1-waiting").is_none(),
        "the answer replaces the mark"
    );
    close(&shell, cx);
}
