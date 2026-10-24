//! The HTML view (html-preview.qml, html-auto-render.qml, html-reload.qml, html-source-refusals.qml): one ask after
//! the settle at the frame's device size, the page decoded off the UI thread, a reload keeping the old page up, Source
//! built once on demand, and every refusal drawing its fixed sentence.
use super::parts::{catalog, draw, look, mark, palette, scale, typeface, with, Calls, METRICS};
use super::{png, root};
use flea::ui_logic_pv2::code_view::{
    Answer, Ask, Reply as CodeReply, Source, BYTE_GATE, REMOTE_GATE,
};
use flea::ui_logic_pv2::html::{Picture, Reply, View, SETTLE_MS};
use flea::ui_logic_pv2::refusal::Refusal;
use flea_gui::g2::pv2::code::{shape, CodeHost};
use flea_gui::g2::pv2::html::{HtmlHost, HtmlView, Metrics, SourceParts};
use flea_gui::g2::pv2::Palette;
use gpui::{hsla, TestAppContext, WindowHandle};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq)]
enum Call {
    Ask(String, String, u32, u32),
    Cancel(String, u32),
}

struct Host(Calls<Call>);

impl HtmlHost for Host {
    fn ask_html(&self, slot: &str, path: &str, width: u32, height: u32) -> u64 {
        self.0
            .borrow_mut()
            .push(Call::Ask(slot.into(), path.into(), width, height));
        self.0.borrow().len() as u64
    }

    fn cancel_html(&self, slot: &str, width: u32) {
        self.0.borrow_mut().push(Call::Cancel(slot.into(), width));
    }
}

struct Code(Calls<Ask>);

impl CodeHost for Code {
    fn ask_code(&self, _: &str, ask: &Ask) -> u64 {
        self.0.borrow_mut().push(ask.clone());
        self.0.borrow().len() as u64
    }
}

const PAGE: Metrics = Metrics {
    row_height: 28.,
    radius: 10.,
    round_top: false,
};

fn open(
    cx: &mut TestAppContext,
    slot: &'static str,
    host: bool,
) -> (WindowHandle<HtmlView>, Calls<Call>, Calls<Ask>) {
    let (calls, code) = (Calls::default(), Calls::default());
    let (html, lines) = (Host(calls.clone()), code.clone());
    let view = cx.add_window(|_, cx| {
        let parts = SourceParts {
            typeface: typeface(),
            palette: palette(),
            metrics: METRICS,
        };
        let ground = Palette {
            background: hsla(0., 0., 0.08, 1.),
        };
        let mut view = HtmlView::new(slot, ground, PAGE, parts, mark("html"), cx);
        let host: Option<Box<dyn HtmlHost>> = if host { Some(Box::new(html)) } else { None };
        view.set_hosts(
            host,
            Some(Box::new(move || {
                Box::new(Code(lines.clone())) as Box<dyn CodeHost>
            })),
        );
        view.set_catalog(catalog(), cx);
        view
    });
    draw(cx, view);
    (view, calls, code)
}

fn show(cx: &mut TestAppContext, view: WindowHandle<HtmlView>, path: &str, size: u64) {
    let path = path.to_owned();
    view.update(cx, |v, _, cx| {
        v.set_active(true, cx);
        v.set_path(&path, size, false, cx);
    })
    .unwrap();
    draw(cx, view);
}

fn settle(cx: &mut TestAppContext, view: WindowHandle<HtmlView>) {
    cx.executor()
        .advance_clock(Duration::from_millis(SETTLE_MS + 1));
    draw(cx, view);
}

fn ready(token: u64, file: &str, width: u32) -> Reply {
    Reply {
        token,
        slot: "look".into(),
        state: "ready".into(),
        file: file.into(),
        total: 61,
        width,
        title: "Page".into(),
    }
}

#[gpui::test]
fn pv2_html_asks_once_after_the_settle_and_decodes_off_the_ui_thread(cx: &mut TestAppContext) {
    let root = root();
    let (view, calls, _) = open(cx, "look", true);
    show(cx, view, "/tmp/page.html", 1_600);
    assert!(
        calls.borrow().is_empty(),
        "nothing is asked before the settle"
    );
    assert!(
        with(cx, view, |v| v.doc.deadline().is_some()),
        "the settle is armed"
    );
    settle(cx, view);
    let asks = calls.borrow().clone();
    assert_eq!(asks.len(), 1, "one ask after the settle: {asks:?}");
    let (frame, width, height) = with(cx, view, |v| {
        (v.frame, v.doc.device_width, v.doc.device_height)
    });
    assert_eq!(
        asks[0],
        Call::Ask("look".into(), "/tmp/page.html".into(), width, height),
        "at the frame's device size"
    );
    assert_eq!(
        width,
        (frame.0 * scale(cx, view)).round() as u32,
        "the frame's width in device pixels"
    );
    assert!(
        look(cx, view, |v, cx| v.loading(cx)),
        "the loading mark shows until the page lands"
    );
    let file = png(&root, "page.png", width, height * 2);
    view.update(cx, |v, _, cx| v.reply(ready(1, &file, width), cx))
        .unwrap();
    draw(cx, view);
    let (decoded, picture, rect, content) = look(cx, view, |v, cx| {
        (v.decoded, v.doc.picture, v.page_rect(), v.content_ready(cx))
    });
    assert_eq!(
        (decoded, picture),
        (1, Picture::Ready),
        "one decode, off the UI thread"
    );
    let rect = rect.expect("the page draws");
    assert_eq!(
        (rect.2, rect.3),
        (frame.0, frame.0 * 2. * height as f32 / width as f32),
        "the page fills the width at its proportion"
    );
    assert!(content, "the content is ready");
    let i18n = catalog();
    let bar = look(cx, view, |v, cx| v.bar(&i18n, cx));
    assert_eq!(
        bar.count.as_deref(),
        Some("61 lines"),
        "the bar counts the helper's lines"
    );
    let words = bar
        .views
        .expect("Quick Look's HTML bar carries the view words");
    assert_eq!(
        (words.rendered.as_str(), words.source.as_str(), words.active),
        ("Rendered", "Source", View::Rendered)
    );
    view.update(cx, |v, _, cx| v.scroll_rows(4, cx)).unwrap();
    assert_eq!(
        with(cx, view, |v| v.scroll_y),
        4. * PAGE.row_height,
        "j scrolls the page by rows"
    );
    draw(cx, view);
    assert_eq!(calls.borrow().len(), 1, "scrolling asks nothing");
    view.update(cx, |v, _, cx| v.scroll_rows(10_000, cx))
        .unwrap();
    assert_eq!(
        with(cx, view, |v| v.end_gap()),
        0.,
        "at the end the page meets the viewport's bottom edge"
    );
    let viewport = png(&root, "viewport.png", width, height);
    view.update(cx, |v, _, cx| v.reload(cx)).unwrap();
    draw(cx, view);
    view.update(cx, |v, _, cx| v.reply(ready(2, &viewport, width), cx))
        .unwrap();
    draw(cx, view);
    let (top, gap) = with(cx, view, |v| (v.scroll_y, v.end_gap()));
    assert_eq!(
        (top, gap),
        (0., 0.),
        "a short page rendered at the viewport height covers its bottom edge (F9)"
    );
}

#[gpui::test]
fn pv2_html_reload_keeps_the_drawn_page_and_drops_a_stale_reply(cx: &mut TestAppContext) {
    let root = root();
    let (view, calls, _) = open(cx, "look", true);
    show(cx, view, "/tmp/page.html", 1_600);
    settle(cx, view);
    let width = with(cx, view, |v| v.doc.device_width);
    let first = png(&root, "first.png", width, 300);
    view.update(cx, |v, _, cx| v.reply(ready(1, &first, width), cx))
        .unwrap();
    draw(cx, view);
    view.update(cx, |v, _, cx| v.reload(cx)).unwrap();
    draw(cx, view);
    assert_eq!(calls.borrow().len(), 2, "a reload asks at once, no settle");
    assert!(
        !look(cx, view, |v, cx| v.loading(cx)),
        "the drawn page stays up with no loading mark"
    );
    assert!(
        with(cx, view, |v| v.page_rect().is_some()),
        "and keeps drawing"
    );
    view.update(cx, |v, _, cx| v.reply(ready(1, &first, width), cx))
        .unwrap();
    draw(cx, view);
    assert_eq!(
        with(cx, view, |v| v.decoded),
        1,
        "the old token's reply is dropped"
    );
    let second = png(&root, "second.png", width, 600);
    view.update(cx, |v, _, cx| v.reply(ready(2, &second, width), cx))
        .unwrap();
    draw(cx, view);
    let (decoded, size) = with(cx, view, |v| {
        (v.decoded, v.image.as_ref().map(|(_, _, size)| *size))
    });
    assert_eq!(
        (decoded, size),
        (2, Some((width, 600))),
        "the new page replaces the old in one swap"
    );
    view.update(cx, |v, _, cx| v.set_path("/tmp/other.html", 900, false, cx))
        .unwrap();
    draw(cx, view);
    assert!(
        look(cx, view, |v, cx| v.image.is_none() && v.loading(cx)),
        "a new subject drops the old page"
    );
}

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
        background: hsla(0., 0., 0.08, 1.),
    };
    let light = Palette {
        background: hsla(0., 0., 0.95, 1.),
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

const NAS: &str = "/run/user/1000/gvfs/smb-share=site/page.html";

// The host's subject: one page and its storage class.
fn subject(
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
fn read(
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

fn source_view(cx: &mut TestAppContext, view: WindowHandle<HtmlView>) {
    view.update(cx, |v, _, cx| v.set_view(View::Source, cx))
        .unwrap();
    draw(cx, view);
}

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
