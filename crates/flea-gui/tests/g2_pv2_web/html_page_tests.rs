//! HTML page loading, reload and decode lifecycle.
use super::*;

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
