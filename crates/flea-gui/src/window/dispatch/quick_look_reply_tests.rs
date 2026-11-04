//! Each answer the open Quick Look card takes or leaves: the reveal cap, stale and foreign replies, pages, refusals and a lost backend.
use super::{
    preview_tests::{
        answer, close, code_reply, context, drain, draw, opened, press, rows, sent, thumbed_reply,
        token,
    },
    test_shell::{land, set_cursor},
};
use crate::{backend::Event as BackendEvent, g2::pv1::Body, state::Shell, window::u07_tests::read};
use flea::{jsondoc::Json, keys::Context};
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::time::Duration;

const CAP: u64 = crate::pane::swap::CAP_MS;

fn page(token: u64, first: usize, count: usize, total: usize) -> String {
    let lines: Vec<String> = (first..first + count)
        .map(|n| format!(r#"[0,"line {n}"]"#))
        .collect();
    format!(
        r#"{{"t":"code","token":{token},"slot":"look","state":"plain","first":{first},"total":{total},"lines":[{}]}}"#,
        lines.join(",")
    )
}

fn refused(token: u64, state: &str) -> String {
    format!(
        r#"{{"t":"code","token":{token},"slot":"look","state":"{state}","first":1,"total":0,"lines":[]}}"#
    )
}

fn meta_reply(token: u64) -> String {
    format!(
        r#"{{"t":"meta","row":2,"w":800,"h":600,"orient":1,"ms":0,"rate":0,"entries":0,"unpacked":0,"afailed":false,"names":[],"lines":0,"partial":false,"lfailed":false,"frames":0,"photo":null,"target":"","targetdir":false,"raw":null,"owner":"gm","token":{token}}}"#
    )
}

fn code(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
) -> Entity<crate::g2::pv2::code::CodeView> {
    shell.read_with(cx, |shell, cx| {
        let host = shell.quick_look.as_ref().expect("the card is open");
        let subject = host
            .view
            .read(cx)
            .subject()
            .expect("the card has a subject");
        let Body::Doc(doc) = &subject.body else {
            panic!("the card mounts a document")
        };
        let crate::g2::pv1::doc::View::Code(code) = &doc.view else {
            panic!("the card mounts CodeView")
        };
        code.clone()
    })
}

fn lines(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Vec<String> {
    code(shell, cx).read_with(cx, |code, _| code.lines.plain_lines())
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
    let revealed = shell.read_with(cx, |shell, cx| {
        shell
            .quick_look
            .as_ref()
            .is_some_and(|host| host.view.read(cx).is_revealed())
    });
    assert!(revealed, "revealed at the cap");
    // EY-447: PreviewText.status stays loading until the lines land, as v0.3.12 reports it.
    assert_eq!(read(&shell, cx, "previewState", &[]), "loading");
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
    // While the code ask is owed: the column's reply with its token, then the card's own slot with an older token.
    for line in [
        format!(
            r#"{{"t":"code","token":{first},"slot":"column","state":"plain","first":1,"total":1,"lines":[["column"]]}}"#
        ),
        code_reply(first - 1, &["older"], 1),
    ] {
        answer(&shell, cx, line);
        assert_eq!(
            read(&shell, cx, "previewState", &[]),
            "loading",
            "the code ask is still owed"
        );
    }
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

// A longer read fetches only the visible pages; scrolling asks for the later lines with a fresh token.
#[gpui::test]
fn a_two_page_read_asks_from_the_next_line_and_joins_the_pages(cx: &mut TestAppContext) {
    let (shell, cx, mut requests) = opened(cx);
    land(&shell, cx, "/files", rows(), 1);
    set_cursor(&shell, cx, 1);
    press(cx, "space");
    let asked = sent(&mut requests, "code");
    assert_eq!(asked.len(), 1, "one initial page");
    let first = asked[0].get("first").and_then(Json::as_f64).unwrap() as usize;
    let count = asked[0].get("count").and_then(Json::as_f64).unwrap() as usize;
    assert_eq!(first, 1);
    assert!(
        count > 0 && count < 401,
        "the first ask is bounded to the view"
    );
    answer(&shell, cx, page(token(&asked[0]), first, count, 401));
    assert_eq!(
        read(&shell, cx, "previewState", &[]),
        "ready",
        "the first page reveals the card"
    );
    let code = code(&shell, cx);
    assert_eq!(code.read_with(cx, |code, _| code.lines.held()), count);
    assert!(
        sent(&mut requests, "code").is_empty(),
        "off-screen lines are not read eagerly"
    );
    code.update(cx, |code, cx| code.scroll_rows(400, cx));
    draw(cx);
    let next = sent(&mut requests, "code");
    assert_eq!(next.len(), 1, "one more page for the scrolled view");
    let later = next[0].get("first").and_then(Json::as_f64).unwrap() as usize;
    let count = next[0].get("count").and_then(Json::as_f64).unwrap() as usize;
    assert!(
        later > first,
        "the later page starts after the original line"
    );
    assert_eq!(
        later + count - 1,
        401,
        "the requested window reaches the file's last line"
    );
    assert_ne!(
        token(&next[0]),
        token(&asked[0]),
        "each page has its own token"
    );
    answer(&shell, cx, page(token(&next[0]), later, count, 401));
    let joined = lines(&shell, cx);
    assert_eq!(joined.len(), 401);
    assert_eq!(
        (joined[0].as_str(), joined[400].as_str()),
        ("line 1", "line 401")
    );
    assert_eq!(
        shell.read_with(cx, |shell, cx| code
            .read(cx)
            .count_label(&shell.locale.catalog)),
        Some("401 lines".into())
    );
    assert!(
        sent(&mut requests, "code").is_empty(),
        "the requested view is whole"
    );
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
            .any(|ask| ask.get("slot").and_then(Json::as_str) == Some("look")),
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

// SelectionPreview.qml: Space in the focused column on a clip leaves Quick Look closed; playback is not driven here.
#[gpui::test]
fn space_in_the_focused_preview_column_on_a_clip_opens_no_card(cx: &mut TestAppContext) {
    use crate::window::{selection_preview::Kind, startup::View};
    let (shell, cx, mut requests) = opened(cx);
    let mut listed = rows();
    listed.push(super::preview_tests::file("clip.mp4", "video-mp4"));
    land(&shell, cx, "/files", listed, 1);
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.set_view(View::Columns, window, cx)));
    draw(cx);
    // The column follows the cursor the Columns keys move.
    for _ in 0..4 {
        press(cx, "down");
    }
    assert_eq!(read(&shell, cx, "cursor", &[]), "4");
    for _ in 0..4 {
        advance(cx, 400);
    }
    // The folder's class, then the column's own meta and thumb answers, land the clip; a clip decodes nothing to mount.
    let local = crate::list::sizes::Reply::Filesystem {
        path: "/files".into(),
        class: "local".into(),
    };
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(0, BackendEvent::ListDetails(local), window, cx)
        })
    });
    advance(cx, 400);
    let asked = drain(&mut requests);
    let generation = asked
        .iter()
        .filter(|ask| ask.get("c").and_then(Json::as_str) == Some("meta"))
        .find(|ask| ask.get("media").and_then(Json::as_bool) == Some(true))
        .map(token)
        .expect("the column asks for the clip's meta");
    answer(&shell, cx, meta_reply(generation));
    answer(&shell, cx, thumbed_reply(&asked, 4, ""));
    for _ in 0..4 {
        advance(cx, 400);
    }
    let column = shell
        .read_with(cx, |shell, _| shell.selection_preview(0))
        .expect("Columns hosts the preview column");
    let decoder = super::previewtypes_tests::Decoder::default();
    cx.update(|_, cx| {
        column.update(cx, |column, _| {
            column.set_media_backend(std::rc::Rc::new(decoder.clone()));
        });
    });
    let video = cx.update(|_, cx| matches!(column.read(cx).mounted_kind(), Some((Kind::Video, _))));
    assert!(video, "the column mounted the clip");
    press(cx, "ctrl-tab");
    assert_eq!(
        context(&shell, cx),
        Context::Media,
        "a mounted clip holds the media keys"
    );
    sent(&mut requests, "code");
    press(cx, "space");
    assert_eq!(
        *decoder.started.borrow(),
        1,
        "Space reaches the in-process decoder"
    );
    assert_eq!(
        read(&shell, cx, "previewOpen", &[]),
        "false",
        "a clip opens no card"
    );
    let quick = sent(&mut requests, "code")
        .iter()
        .any(|ask| ask.get("slot").and_then(Json::as_str) == Some("look"));
    assert!(!quick, "no quicklook ask for the clip");
    close(&shell, cx);
}
