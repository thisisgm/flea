//! Each answer the open Quick Look card takes or leaves: the reveal cap, stale and foreign replies, pages, refusals and a lost backend.
use super::{
    preview_tests::{answer, close, code_reply, context, draw, opened, press, rows, sent, token},
    test_shell::{land, set_cursor},
};
use crate::{backend::Event as BackendEvent, g2::pv1::Body, state::Shell, window::u07_tests::read};
use flea::{jsondoc::Json, keys::Context};
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::time::Duration;

const CAP: u64 = crate::pane::swap::CAP_MS;

fn page(token: u64, first: usize, count: usize, total: usize) -> String {
    let lines: Vec<String> = (first..first + count)
        .map(|n| format!(r#"["line {n}"]"#))
        .collect();
    format!(
        r#"{{"t":"code","token":{token},"slot":"quicklook","state":"plain","first":{first},"total":{total},"lines":[{}]}}"#,
        lines.join(",")
    )
}

fn refused(token: u64, state: &str) -> String {
    format!(
        r#"{{"t":"code","token":{token},"slot":"quicklook","state":"{state}","first":1,"total":0,"lines":[]}}"#
    )
}

fn meta_reply(token: u64) -> String {
    format!(
        r#"{{"t":"meta","row":2,"w":800,"h":600,"orient":1,"ms":0,"rate":0,"entries":0,"unpacked":0,"afailed":false,"names":[],"lines":0,"partial":false,"lfailed":false,"frames":0,"photo":null,"target":"","targetdir":false,"raw":null,"owner":"gm","token":{token}}}"#
    )
}

fn lines(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Vec<String> {
    shell.read_with(cx, |shell, cx| {
        match shell
            .quick_look
            .as_ref()
            .and_then(|h| h.view.read(cx).subject())
        {
            Some(subject) => match &subject.body {
                Body::Text { lines, .. } => lines.clone(),
                _ => Vec::new(),
            },
            None => Vec::new(),
        }
    })
}

fn advance(cx: &mut VisualTestContext, ms: u64) {
    cx.executor().advance_clock(Duration::from_millis(ms));
    draw(cx);
}

// PreviewSwap's cap: an answer that has not landed by CAP_MS reveals the card with what it has, and a follow restarts the cap.
#[gpui::test]
fn an_unanswered_card_reveals_at_the_cap_and_a_follow_restarts_it(cx: &mut TestAppContext) {
    let (shell, cx, mut requests) = opened(cx);
    land(&shell, cx, "/files", rows(), 1);
    set_cursor(&shell, cx, 1);
    press(cx, "space");
    assert_eq!(read(&shell, cx, "previewState", &[]), "loading");
    advance(cx, CAP - 10);
    assert_eq!(
        read(&shell, cx, "previewState", &[]),
        "loading",
        "before the cap"
    );
    advance(cx, 10);
    assert_eq!(
        read(&shell, cx, "previewState", &[]),
        "text",
        "revealed at the cap"
    );
    assert_eq!(sent(&mut requests, "code").len(), 1);
    press(cx, "escape");
    // Open, wait part of the cap, follow onto the picture: the first cap passing reveals nothing.
    press(cx, "space");
    advance(cx, CAP - 26);
    press(cx, "down");
    assert_eq!(sent(&mut requests, "meta").len(), 1, "the card followed");
    advance(cx, 30);
    assert_eq!(
        read(&shell, cx, "previewState", &[]),
        "loading",
        "the first file's cap is gone"
    );
    advance(cx, CAP - 30);
    assert_eq!(read(&shell, cx, "previewState", &[]), "image");
    close(&shell, cx);
}

// A reply for a superseded ask, or one the column asked for, never lands on the card.
#[gpui::test]
fn a_stale_or_column_reply_is_left_alone(cx: &mut TestAppContext) {
    let (shell, cx, mut requests) = opened(cx);
    land(&shell, cx, "/files", rows(), 1);
    set_cursor(&shell, cx, 1);
    press(cx, "space");
    let first = token(&sent(&mut requests, "code")[0]);
    press(cx, "down");
    let meta = token(&sent(&mut requests, "meta")[0]);
    answer(&shell, cx, code_reply(first, &["late"], 1));
    assert_eq!(read(&shell, cx, "previewKind", &[]), "image");
    assert_eq!(
        read(&shell, cx, "previewState", &[]),
        "loading",
        "the meta ask is still owed"
    );
    let column = format!(
        r#"{{"t":"code","token":{meta},"slot":"column","state":"plain","first":1,"total":1,"lines":[["column"]]}}"#
    );
    answer(&shell, cx, column);
    assert_eq!(
        read(&shell, cx, "previewState", &[]),
        "loading",
        "a column reply is not the card's"
    );
    answer(&shell, cx, meta_reply(first));
    assert_eq!(
        read(&shell, cx, "previewState", &[]),
        "loading",
        "a meta reply for another token"
    );
    answer(&shell, cx, meta_reply(meta));
    assert_eq!(read(&shell, cx, "previewState", &[]), "image");
    close(&shell, cx);
}

// A read longer than one page asks for the next page from the first line it lacks and joins the two.
#[gpui::test]
fn a_two_page_read_asks_from_the_next_line_and_joins_the_pages(cx: &mut TestAppContext) {
    let (shell, cx, mut requests) = opened(cx);
    land(&shell, cx, "/files", rows(), 1);
    set_cursor(&shell, cx, 1);
    press(cx, "space");
    let asked = sent(&mut requests, "code");
    assert_eq!(asked[0].get("first").and_then(Json::as_f64), Some(1.));
    answer(&shell, cx, page(token(&asked[0]), 1, 400, 401));
    assert_eq!(
        read(&shell, cx, "previewState", &[]),
        "text",
        "the first page reveals the card"
    );
    assert_eq!(lines(&shell, cx).len(), 400);
    let next = sent(&mut requests, "code");
    assert_eq!(next.len(), 1, "one more page");
    assert_eq!(next[0].get("first").and_then(Json::as_f64), Some(401.));
    assert_ne!(
        token(&next[0]),
        token(&asked[0]),
        "each page has its own token"
    );
    answer(&shell, cx, page(token(&next[0]), 401, 1, 401));
    let joined = lines(&shell, cx);
    assert_eq!(joined.len(), 401);
    assert_eq!(
        (joined[0].as_str(), joined[400].as_str()),
        ("line 1", "line 401")
    );
    assert!(
        matches!(super::preview_tests::body(&shell, cx), Some(Body::Text { count, .. }) if count == "401 lines")
    );
    assert!(sent(&mut requests, "code").is_empty(), "the read is whole");
    close(&shell, cx);
}

// The backend's refused reads each name their reason on the card.
#[gpui::test]
fn a_large_binary_or_failed_read_says_why(cx: &mut TestAppContext) {
    let (shell, cx, mut requests) = opened(cx);
    land(&shell, cx, "/files", rows(), 1);
    set_cursor(&shell, cx, 1);
    for (state, sentence) in [
        ("large", "This file is too large to preview."),
        ("binary", "This file cannot be previewed."),
        ("failed", "This file could not be read."),
    ] {
        press(cx, "space");
        let asked = sent(&mut requests, "code");
        answer(&shell, cx, refused(token(&asked[0]), state));
        assert_eq!(read(&shell, cx, "previewState", &[]), sentence, "{state}");
        assert!(
            sent(&mut requests, "code").is_empty(),
            "{state}: no further page"
        );
        press(cx, "escape");
    }
    close(&shell, cx);
}

// A backend that drains with a read still owed leaves the card saying the file could not be read.
#[gpui::test]
fn a_drained_backend_mid_read_says_the_file_could_not_be_read(cx: &mut TestAppContext) {
    let (shell, cx, _requests) = opened(cx);
    land(&shell, cx, "/files", rows(), 1);
    set_cursor(&shell, cx, 1);
    press(cx, "space");
    assert_eq!(read(&shell, cx, "previewState", &[]), "loading");
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(0, BackendEvent::Drained, window, cx)
        })
    });
    draw(cx);
    assert_eq!(read(&shell, cx, "previewOpen", &[]), "true");
    assert_eq!(
        read(&shell, cx, "previewState", &[]),
        "This file could not be read."
    );
    close(&shell, cx);
}

// SelectionPreview.qml: Space in the focused Columns preview opens Quick Look, and Escape hands the keyboard back to the column.
#[gpui::test]
fn space_in_the_focused_preview_column_opens_the_card_and_escape_returns_to_it(
    cx: &mut TestAppContext,
) {
    use crate::window::startup::View;
    let (shell, cx, mut requests) = opened(cx);
    land(&shell, cx, "/files", rows(), 1);
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.set_view(View::Columns, window, cx)));
    draw(cx);
    set_cursor(&shell, cx, 1);
    // The column's track mounts once the swap that follows the cursor has had its time.
    for _ in 0..4 {
        advance(cx, 400);
    }
    let column = shell
        .read_with(cx, |shell, _| shell.selection_preview(0))
        .expect("Columns hosts the preview column");
    // focusPreview: Ctrl+Tab hands the keyboard to the shown column.
    press(cx, "ctrl-tab");
    let focused = cx.update(|window, cx| column.read(cx).focus.is_focused(window));
    assert!(focused, "the column holds the keyboard");
    assert_eq!(context(&shell, cx), Context::Preview);
    sent(&mut requests, "code");
    press(cx, "space");
    assert_eq!(
        read(&shell, cx, "previewOpen", &[]),
        "true",
        "Space opens the card"
    );
    let asked = sent(&mut requests, "code");
    assert!(
        asked
            .iter()
            .any(|ask| ask.get("slot").and_then(Json::as_str) == Some("quicklook")),
        "the card asks for the cursor file"
    );
    press(cx, "escape");
    assert_eq!(read(&shell, cx, "previewOpen", &[]), "false");
    assert_eq!(
        context(&shell, cx),
        Context::Preview,
        "the column's context comes back"
    );
    let focused = cx.update(|window, cx| column.read(cx).focus.is_focused(window));
    assert!(focused, "the column has the keyboard again");
    close(&shell, cx);
}
