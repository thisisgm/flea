//! Remote HTML Source transport and stale-reply isolation.
use super::source::{read, source_view, subject, NAS};
use super::*;

#[gpui::test]
fn pv2_html_remote_source_never_sends_the_nas_path(cx: &mut TestAppContext) {
    let (view, _, code) = open(cx, "look", true);
    subject(cx, view, "/tmp/local.html", 2_000, false);
    source_view(cx, view);
    let local = code.borrow().len();
    assert!(
        local > 0
            && code
                .borrow()
                .iter()
                .all(|ask| ask.source == Source::Path("/tmp/local.html".into())),
        "a local page's Source asks by path: {:?}",
        code.borrow()
    );
    let nas = subject(cx, view, NAS, 2_000, true);
    let moved = code.borrow()[local..].to_vec();
    assert!(
        moved.is_empty(),
        "moving to a NAS page asks nothing until the caller's read lands, never the path: {moved:?}"
    );
    assert_eq!(
        with(cx, view, |v| v.doc.max_bytes),
        REMOTE_GATE,
        "the page takes the remote gate with its storage (Preview.qml maxBytes)"
    );
    read(cx, view, nas, Some("<p>nas</p>\n"), false);
    let read_asks = code.borrow()[local..].to_vec();
    assert!(
        !read_asks.is_empty()
            && read_asks.iter().all(
                |ask| matches!(&ask.source, Source::Text { text, .. } if text.contains("nas"))
            ),
        "the caller's bytes are asked to be coloured: {read_asks:?}"
    );
    let seen = code.borrow().len();
    subject(cx, view, "/tmp/back.html", 2_000, false);
    let back = code.borrow()[seen..].to_vec();
    assert!(
        !back.is_empty()
            && back
                .iter()
                .all(|ask| ask.source == Source::Path("/tmp/back.html".into())),
        "a local page after it asks by its own path, not the NAS bytes: {back:?}"
    );
    assert_eq!(
        with(cx, view, |v| v.doc.max_bytes),
        BYTE_GATE,
        "and takes the local gate back"
    );
}

#[gpui::test]
fn pv2_html_remote_source_never_draws_the_previous_pages_bytes(cx: &mut TestAppContext) {
    let (view, _, code) = open(cx, "look", true);
    let (a, b, c) = (
        "/run/user/1000/gvfs/smb-share=site/a.html",
        "/run/user/1000/gvfs/smb-share=site/b.html",
        "/run/user/1000/gvfs/smb-share=site/c.html",
    );
    let a1 = subject(cx, view, a, 2_000, true);
    source_view(cx, view);
    read(cx, view, a1, Some("<p>page A secret</p>\n"), false);
    assert!(
        code.borrow()
            .iter()
            .any(|ask| matches!(&ask.source, Source::Text { text, .. } if text.contains("page A"))),
        "page A's read is drawn under A"
    );
    let seen = code.borrow().len();
    let b1 = subject(cx, view, b, 2_000, true);
    read(cx, view, a1, Some("<p>late A</p>\n"), false);
    let moved = code.borrow()[seen..].to_vec();
    assert!(
        moved.is_empty(),
        "page B asks nothing with page A's bytes, nor with a late read of A: {moved:?}"
    );
    let source = with(cx, view, |v| v.source.clone()).expect("Source built");
    assert_eq!(
        source.read_with(cx, |code, _| code.lines.held()),
        0,
        "and draws none of A's lines under B"
    );
    read(cx, view, b1, Some("<p>page B</p>\n"), false);
    let own = code.borrow()[seen..].to_vec();
    assert!(
        !own.is_empty()
            && own.iter().all(|ask| matches!(
                &ask.source,
                Source::Text { text, name } if text.contains("page B") && name == "b.html"
            )),
        "B shows its own bytes once its read lands: {own:?}"
    );
    let i18n = catalog();
    read(cx, view, b1, None, true);
    assert_eq!(
        look(cx, view, |v, cx| v.fail_sentence(&i18n, cx)).as_deref(),
        Some("This file could not be read."),
        "B's failed read is B's failure"
    );
    subject(cx, view, c, 2_000, true);
    assert_eq!(
        look(cx, view, |v, cx| v.fail_sentence(&i18n, cx)),
        None,
        "and is not carried to the next page"
    );
}

#[gpui::test]
fn pv2_html_unchanged_frame_flushes_and_shapes_nothing(cx: &mut TestAppContext) {
    let (view, _, _) = open(cx, "look", true);
    show(cx, view, "/tmp/page.html", 1_600);
    settle(cx, view);
    let counts = |cx: &mut TestAppContext| with(cx, view, |v| (v.flushes, shape::shaped_bytes()));
    let before = counts(cx);
    for _ in 0..3 {
        draw(cx, view);
    }
    assert_eq!(
        counts(cx),
        before,
        "three idle redraws of a page flush nothing and shape nothing (flushes, bytes)"
    );
    show(cx, view, "/tmp/huge.html", BYTE_GATE + 1);
    assert!(
        with(cx, view, |v| v.doc.over_size()),
        "the too-large frame shows"
    );
    let before = counts(cx);
    for _ in 0..3 {
        draw(cx, view);
    }
    assert_eq!(
        counts(cx),
        before,
        "three idle redraws of the too-large frame shape its caption no more"
    );
}

fn drew(asks: &[Ask], needle: &str) -> bool {
    asks.iter()
        .any(|ask| matches!(&ask.source, Source::Text { text, .. } if text.contains(needle)))
}

#[gpui::test]
fn pv2_html_superseded_read_of_a_returned_page_is_dropped(cx: &mut TestAppContext) {
    let a = "/run/user/1000/gvfs/smb-share=site/a.html";
    let b = "/run/user/1000/gvfs/smb-share=site/b.html";
    let i18n = catalog();
    // The superseded read lands after the current one, with a late transient failure.
    let (view, _, code) = open(cx, "look", true);
    let a1 = subject(cx, view, a, 2_000, true);
    source_view(cx, view);
    subject(cx, view, b, 2_000, true);
    let a2 = subject(cx, view, a, 2_000, true);
    read(cx, view, a2, Some("<p>A as read by A2</p>\n"), false);
    read(cx, view, a1, Some("<p>A as read by A1</p>\n"), false);
    read(cx, view, a1, None, true);
    let asks = code.borrow().clone();
    assert!(
        drew(&asks, "A as read by A2"),
        "the current read is drawn: {asks:?}"
    );
    assert!(
        !drew(&asks, "A as read by A1"),
        "a superseded read landing last is never drawn: {asks:?}"
    );
    assert_eq!(
        look(cx, view, |v, cx| v.fail_sentence(&i18n, cx)),
        None,
        "and a late failure of it does not replace the good read"
    );
    // The superseded read lands first: it is dropped, and the current one still draws.
    let (view, _, code) = open(cx, "look", true);
    let a1 = subject(cx, view, a, 2_000, true);
    source_view(cx, view);
    subject(cx, view, b, 2_000, true);
    let a2 = subject(cx, view, a, 2_000, true);
    read(cx, view, a1, Some("<p>A as read by A1</p>\n"), false);
    let early = code.borrow().clone();
    assert!(
        !drew(&early, "A as read by A1"),
        "a superseded read landing first is never drawn: {early:?}"
    );
    read(cx, view, a2, Some("<p>A as read by A2</p>\n"), false);
    let late = code.borrow().clone();
    assert!(
        drew(&late, "A as read by A2"),
        "the current read draws once it lands: {late:?}"
    );
}

#[gpui::test]
fn pv2_html_too_large_caption_reshapes_on_every_catalog_change(cx: &mut TestAppContext) {
    let (view, _, _) = open(cx, "look", true);
    show(cx, view, "/tmp/huge.html", BYTE_GATE + 1);
    for _ in 0..6 {
        let before = shape::shaped_bytes();
        view.update(cx, |v, _, cx| v.set_catalog(catalog(), cx))
            .unwrap();
        draw(cx, view);
        assert!(
            shape::shaped_bytes() > before,
            "a new catalog reshapes the too-large caption"
        );
    }
    // Two catalog changes with no frame laid out between them, the freed address reused.
    let before = shape::shaped_bytes();
    view.update(cx, |v, _, cx| v.set_catalog(catalog(), cx))
        .unwrap();
    view.update(cx, |v, _, cx| v.set_catalog(catalog(), cx))
        .unwrap();
    draw(cx, view);
    assert!(
        shape::shaped_bytes() > before,
        "two catalog changes with no frame between them still reshape the caption"
    );
    let before = shape::shaped_bytes();
    draw(cx, view);
    assert_eq!(
        shape::shaped_bytes(),
        before,
        "and an unchanged catalog reshapes nothing"
    );
}
