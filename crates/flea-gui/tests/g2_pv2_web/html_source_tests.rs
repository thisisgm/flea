//! HTML Source mounting, refusals and page geometry.
use super::*;

#[gpui::test]
fn pv2_html_source_builds_its_code_view_once_and_asks_nothing_of_the_helper(
    cx: &mut TestAppContext,
) {
    let (view, calls, code) = open(cx, "look", true);
    show(cx, view, "/tmp/page.html", 1_600);
    assert!(
        with(cx, view, |v| v.source.is_none()),
        "Source is not built until it shows"
    );
    view.update(cx, |v, _, cx| v.set_view(View::Source, cx))
        .unwrap();
    settle(cx, view);
    assert!(calls.borrow().is_empty(), "Source asks the helper nothing");
    assert_eq!(code.borrow().len(), 1, "Source's lines are one code ask");
    let ask = code.borrow()[0].clone();
    let source = with(cx, view, |v| v.source.clone()).expect("Source built");
    source.update(cx, |code, cx| {
        let lines = (ask.first..ask.first + ask.count.min(61))
            .map(|n| vec![(0, format!("<p>{n}</p>"))])
            .collect();
        code.reply(
            CodeReply {
                token: 1,
                state: Answer::Styled,
                first: ask.first,
                total: 61,
                lines,
            },
            cx,
        );
    });
    draw(cx, view);
    let i18n = catalog();
    let bar = look(cx, view, |v, cx| v.bar(&i18n, cx));
    assert_eq!(bar.count.as_deref(), Some("61 lines"), "Source's own count");
    assert_eq!(
        bar.views.map(|words| words.active),
        Some(View::Source),
        "Source is the active word"
    );
    view.update(cx, |v, _, cx| v.set_view(View::Rendered, cx))
        .unwrap();
    view.update(cx, |v, _, cx| v.set_view(View::Source, cx))
        .unwrap();
    draw(cx, view);
    let same = with(cx, view, |v| v.source.clone()).expect("still built");
    assert_eq!(
        same.entity_id(),
        source.entity_id(),
        "flipping back reuses the one code view"
    );
}

#[gpui::test]
fn pv2_html_refusals_draw_their_fixed_sentences(cx: &mut TestAppContext) {
    let i18n = catalog();
    let (view, calls, _) = open(cx, "look", true);
    show(cx, view, "/tmp/huge.html", BYTE_GATE + 1);
    settle(cx, view);
    assert!(calls.borrow().is_empty(), "an oversize page asks nothing");
    let got = look(cx, view, |v, cx| {
        (
            v.refusal(cx),
            v.status(&i18n, cx),
            v.fail_sentence(&i18n, cx),
        )
    });
    assert_eq!(
        got,
        (
            Some(Refusal::TooLarge),
            "This file is too large to preview.".to_owned(),
            None
        ),
        "Quick Look draws the frame"
    );
    let (view, _, _) = open(cx, "look", false);
    show(cx, view, "/tmp/page.html", 1_600);
    settle(cx, view);
    let sentence = look(cx, view, |v, cx| v.fail_sentence(&i18n, cx));
    assert_eq!(
        sentence.as_deref(),
        Some("This file could not be read."),
        "no html route reads as unreadable, never loading forever"
    );
    let (view, _, _) = open(cx, "look", true);
    show(cx, view, "/tmp/page.html", 1_600);
    settle(cx, view);
    let failed = Reply {
        state: "failed".into(),
        ..ready(1, "", 0)
    };
    view.update(cx, |v, _, cx| v.reply(failed, cx)).unwrap();
    assert_eq!(
        look(cx, view, |v, cx| v.fail_sentence(&i18n, cx)).as_deref(),
        Some("This file could not be read."),
        "a failed render"
    );
    let root = root();
    let (view, _, _) = open(cx, "look", true);
    show(cx, view, "/tmp/page.html", 1_600);
    settle(cx, view);
    let broken = root.join("broken.png");
    std::fs::write(&broken, b"not a png").unwrap();
    view.update(cx, |v, _, cx| {
        v.reply(ready(1, broken.to_str().unwrap(), 10), cx)
    })
    .unwrap();
    draw(cx, view);
    assert_eq!(
        look(cx, view, |v, cx| v.refusal(cx)),
        Some(Refusal::Unreadable),
        "an undecodable page is unreadable"
    );
}

#[gpui::test]
fn pv2_html_veil_covers_the_page_in_a_dark_theme_only(cx: &mut TestAppContext) {
    let dark = Palette {
        background: flea_gui::palette::Theme::from_text("background='#141414'").background,
    };
    let light = Palette {
        background: flea_gui::palette::Theme::from_text("background='#f2f2f2'").background,
    };
    assert_eq!(
        dark.veil().map(|veil| veil.a),
        Some(0.05),
        "a 5% veil of the window ground in a dark theme"
    );
    assert_eq!(light.veil(), None, "none in a light one");
    let (view, _, _) = open(cx, "column", true);
    let corners = with(cx, view, |v| v.corners());
    assert_eq!(
        f32::from(corners.bottom_left),
        PAGE.radius,
        "the page clips to the frame's inner arc"
    );
}

#[gpui::test]
fn pv2_html_source_on_remote_storage_draws_the_callers_bytes(cx: &mut TestAppContext) {
    let (view, _, code) = open(cx, "look", true);
    let page = "/run/user/1000/gvfs/smb-share/site/page.html";
    let gen = subject(cx, view, page, 2_000, true);
    view.update(cx, |v, _, cx| v.set_view(View::Source, cx))
        .unwrap();
    draw(cx, view);
    assert!(
        code.borrow().is_empty(),
        "nothing is asked while the caller's read is out"
    );
    view.update(cx, |v, _, cx| {
        v.set_source_text(gen, Some("<p>one</p>\n<p>two</p>\n".into()), false, cx)
    })
    .unwrap();
    draw(cx, view);
    let asks = code.borrow().clone();
    assert!(!asks.is_empty(), "the read lines are asked to be coloured");
    assert!(
        asks.iter()
            .all(|ask| matches!(ask.source, Source::Text { .. })),
        "never by path: {asks:?}"
    );
    let source = with(cx, view, |v| v.source.clone()).unwrap();
    assert_eq!(
        source.read_with(cx, |code, _| code.max_bytes),
        REMOTE_GATE,
        "under the remote gate"
    );
    view.update(cx, |v, _, cx| v.set_source_text(gen, None, true, cx))
        .unwrap();
    let i18n = catalog();
    let sentence = look(cx, view, |v, cx| v.fail_sentence(&i18n, cx));
    assert_eq!(
        sentence.as_deref(),
        Some("This file could not be read."),
        "a failed remote read"
    );
}

#[gpui::test]
fn pv2_html_zoomed_page_pans_sideways(cx: &mut TestAppContext) {
    let root = root();
    let (view, _, _) = open(cx, "look", true);
    show(cx, view, "/tmp/page.html", 1_600);
    settle(cx, view);
    let (width, height) = with(cx, view, |v| (v.doc.device_width, v.doc.device_height));
    let file = png(&root, "page.png", width, height);
    view.update(cx, |v, _, cx| v.reply(ready(1, &file, width), cx))
        .unwrap();
    draw(cx, view);
    view.update(cx, |v, _, cx| {
        v.zoom_by(2, cx);
        v.pan_by(10_000., cx);
    })
    .unwrap();
    draw(cx, view);
    let (rect, frame) = with(cx, view, |v| (v.page_rect().unwrap(), v.frame));
    assert!(rect.2 > frame.0, "the zoomed page is wider than the frame");
    assert_eq!(
        rect.0,
        frame.0 - rect.2,
        "panning reaches the page's right edge and stops there"
    );
    view.update(cx, |v, _, cx| v.zoom_fit(cx)).unwrap();
    assert_eq!(
        with(cx, view, |v| v.page_rect().unwrap().0),
        0.,
        "fit returns to the left edge"
    );
}

pub(super) const NAS: &str = "/run/user/1000/gvfs/smb-share=site/page.html";

// The host's subject: one page and its storage class.
pub(super) fn subject(
    cx: &mut TestAppContext,
    view: WindowHandle<HtmlView>,
    path: &str,
    size: u64,
    remote: bool,
) -> u64 {
    let path = path.to_owned();
    let generation = view
        .update(cx, |v, _, cx| {
            v.set_active(true, cx);
            v.set_path(&path, size, remote, cx)
        })
        .unwrap();
    draw(cx, view);
    generation
}

// The host's own read of a remote page landing, quoting the generation of the set_path it was read for.
pub(super) fn read(
    cx: &mut TestAppContext,
    view: WindowHandle<HtmlView>,
    generation: u64,
    text: Option<&str>,
    failed: bool,
) {
    let text = text.map(str::to_owned);
    view.update(cx, |v, _, cx| {
        v.set_source_text(generation, text, failed, cx)
    })
    .unwrap();
    draw(cx, view);
}

pub(super) fn source_view(cx: &mut TestAppContext, view: WindowHandle<HtmlView>) {
    view.update(cx, |v, _, cx| v.set_view(View::Source, cx))
        .unwrap();
    draw(cx, view);
}
