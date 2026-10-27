//! A refused code read draws the calm can't-show card (ql-unsupported): the binary sentence or the too-large caption, with Open With under it.
use super::{
    preview_tests::{answer, close, code_reply, draw, file, opened, press, sent, token, Lines},
    test_shell::{land, row, set_cursor},
};
use crate::{
    g2::pv1::{Body, Hit},
    state::Shell,
    window::u07_tests::read,
};
use flea::{jsondoc::Json, protocol::RowFacts};
use gpui::{Entity, TestAppContext, VisualTestContext};

// A text icon with no known extension: the card mounts the code view and learns from the backend that the read is binary or large.
const NAME: &str = "unsupported.flea-unknown";

fn rows() -> Vec<RowFacts> {
    vec![row("alpha", true), file(NAME, "text-plain")]
}

fn refused(token: u64, state: &str) -> String {
    format!(
        r#"{{"t":"code","token":{token},"slot":"look","state":"{state}","first":1,"total":0,"lines":[]}}"#
    )
}

// Opens the card on the unknown file and answers its read with `state`.
fn refused_card<'a>(
    cx: &'a mut TestAppContext,
    state: &str,
) -> (Entity<Shell>, &'a mut VisualTestContext, Lines) {
    let (shell, cx, mut requests) = opened(cx);
    land(&shell, cx, "/files", rows(), 1);
    set_cursor(&shell, cx, 1);
    press(cx, "space");
    let asked = sent(&mut requests, "code");
    answer(&shell, cx, refused(token(&asked[0]), state));
    (shell, cx, requests)
}

// The body the card draws in the refusal's place; None when the mounted view draws itself.
fn calm(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Option<Body> {
    shell.read_with(cx, |shell, cx| {
        let host = shell.quick_look.as_ref()?;
        host.view.read(cx).calm_body(cx)
    })
}

#[gpui::test]
fn a_binary_read_draws_the_calm_card_with_its_sentence(cx: &mut TestAppContext) {
    let (shell, cx, _requests) = refused_card(cx, "binary");
    assert_eq!(
        calm(&shell, cx),
        Some(Body::CannotShow {
            sentence: "This file cannot be previewed.".into(),
            error: false,
            controls: Vec::new(),
        }),
        "a binary read draws ql-unsupported's sentence in the calm ink, with no bar"
    );
    assert_eq!(
        read(&shell, cx, "previewState", &[]),
        "This file cannot be previewed.",
        "the status keeps its sentence"
    );
    close(&shell, cx);
}

#[gpui::test]
fn a_large_read_draws_v012s_too_large_caption_in_the_calm_card(cx: &mut TestAppContext) {
    let (shell, cx, _requests) = refused_card(cx, "large");
    assert_eq!(
        calm(&shell, cx),
        Some(Body::CannotShow {
            sentence: "too large to preview".into(),
            error: false,
            controls: Vec::new(),
        }),
        "a large read keeps the v0.3.12 Quick Look caption (CodeRefusal.qml)"
    );
    assert_eq!(
        read(&shell, cx, "previewState", &[]),
        "This file is too large to preview.",
        "the status keeps its sentence"
    );
    close(&shell, cx);
}

#[gpui::test]
fn a_plain_read_keeps_the_text_card(cx: &mut TestAppContext) {
    let (shell, cx, mut requests) = opened(cx);
    land(&shell, cx, "/files", rows(), 1);
    set_cursor(&shell, cx, 1);
    press(cx, "space");
    let asked = sent(&mut requests, "code");
    answer(&shell, cx, code_reply(token(&asked[0]), &["hello"], 1));
    draw(cx);
    assert_eq!(
        calm(&shell, cx),
        None,
        "a readable file is never the calm card"
    );
    close(&shell, cx);
}

#[gpui::test]
fn open_with_on_the_calm_card_opens_the_chooser_for_its_file(cx: &mut TestAppContext) {
    let (shell, cx, mut requests) = refused_card(cx, "binary");
    let view = shell.read_with(cx, |shell, _| {
        shell
            .quick_look
            .as_ref()
            .expect("the card is open")
            .view
            .clone()
    });
    cx.update(|_, cx| view.update(cx, |ql, cx| ql.activate(Hit::OpenWith, cx)));
    draw(cx);
    let snapshots: Vec<Json> = sent(&mut requests, "menuaction")
        .into_iter()
        .filter(|line| line.get("op").and_then(Json::as_str) == Some("snapshot"))
        .collect();
    assert_eq!(
        snapshots.len(),
        1,
        "Open With snapshots the file under the card, as the row menu's Open With does"
    );
    assert_eq!(
        snapshots[0]
            .get("rows")
            .and_then(Json::as_array)
            .map(|rows| rows.len()),
        Some(1),
        "the snapshot names the one row"
    );
    assert!(
        shell.read_with(cx, |shell, _| shell.open_with.is_some()),
        "the Open With chooser opens"
    );
    close(&shell, cx);
}
