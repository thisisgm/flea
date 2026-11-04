//! EY-502: a Compare whose diff is still out reads as loading once the hold passes, never as an empty result.
use super::{
    compare_tests::pair,
    preview_tests::{answer, draw, opened, sent, token},
};
use crate::{pane::swap::HOLD_MS, window::u07_tests::read};
use std::time::Duration;

// v0.3.12 CompareView.qml:33: Preview's status stays "loading" until the backend answers the diff.
#[gpui::test]
fn fixcards_ey502_a_held_compare_reply_reads_loading_after_the_hold(cx: &mut gpui::TestAppContext) {
    let (shell, cx, mut requests) = opened(cx);
    pair(&shell, cx);
    let opened =
        cx.update(|window, cx| shell.update(cx, |shell, cx| shell.open_compare(0, window, cx)));
    assert!(opened, "two selected regular files open Compare");
    draw(cx);
    let asked = sent(&mut requests, "compare");
    assert_eq!(asked.len(), 1, "one diff asked per open");
    cx.executor().advance_clock(Duration::from_millis(HOLD_MS));
    draw(cx);
    assert_eq!(read(&shell, cx, "previewKind", &[]), "compare");
    assert_eq!(
        read(&shell, cx, "previewState", &[]),
        "loading",
        "past HOLD_MS with the diff still out, the card shows its loading state, not an empty refusal"
    );
    let line = format!(
        r#"{{"t":"compare","token":{},"state":"identical","patch":""}}"#,
        token(&asked[0])
    );
    answer(&shell, cx, line);
    assert_eq!(
        read(&shell, cx, "previewState", &[]),
        "The two files are identical.",
        "the answer replaces the loading state"
    );
}
