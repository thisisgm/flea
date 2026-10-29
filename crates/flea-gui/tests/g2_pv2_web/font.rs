//! The font page (fontpane.qml, fontfacts.js): one ask per change of path, colours or sizes, the three samples decoded
//! together off the UI thread, the old samples kept through a theme change, and a refused font drawing one sentence.
use super::parts::{catalog, draw, mark, scale, with, Calls};
use super::{png, root};
use flea::ui_logic_pv2::font::{physical, Ask, Reply, SAMPLE_SIZES, SAMPLE_TEXT};
use flea::ui_logic_pv2::refusal::Refusal;
use flea_gui::g2::pv2::font::{FontHost, FontView, Metrics, Theme};
use gpui::AppContext as _;
use gpui::{px, TestAppContext, WindowHandle};

struct Host(Calls<Ask>);

impl FontHost for Host {
    fn font_look(&self, ask: &Ask) {
        self.0.borrow_mut().push(ask.clone());
    }
}

fn theme(foreground: &str) -> Theme {
    Theme {
        foreground: foreground.into(),
        surface: "#1a1b26".into(),
        muted: flea_gui::palette::Theme::default().muted,
        size_ratio: 1.,
    }
}

fn open(cx: &mut TestAppContext) -> (WindowHandle<FontView>, Calls<Ask>) {
    let calls = Calls::default();
    let host = Host(calls.clone());
    let view = cx.add_window(|_, cx| {
        let metrics = Metrics {
            font: gpui::font("sans-serif"),
            caption: px(12.),
        };
        let mut view = FontView::new(theme("#c0caf5"), metrics, mark("font"));
        view.set_host(Some(Box::new(host)));
        view.set_catalog(catalog(), cx);
        view
    });
    view.update(cx, |v, _, cx| {
        v.set_active(true, cx);
        v.set_subject("/tmp/Inter-Bold.ttf", 312_000, cx);
    })
    .unwrap();
    draw(cx, view);
    (view, calls)
}

fn samples(root: &super::Root, tag: &str) -> Vec<String> {
    SAMPLE_SIZES
        .iter()
        .map(|size| png(root, &format!("{tag}-{size}.png"), 40 * size, 2 * size))
        .collect()
}

fn answer(
    cx: &mut TestAppContext,
    view: WindowHandle<FontView>,
    id: u64,
    files: Vec<String>,
    err: Option<&str>,
) {
    let reply = Reply {
        id,
        err: err.map(String::from),
        family: "Inter".into(),
        style: "Bold".into(),
        glyphs: 2548,
        files,
    };
    view.update(cx, |v, _, cx| v.answered(reply, cx)).unwrap();
    draw(cx, view);
}

#[gpui::test]
fn pv2_font_asks_once_and_draws_three_samples_together(cx: &mut TestAppContext) {
    let root = root();
    let (view, calls) = open(cx);
    draw(cx, view);
    assert_eq!(calls.borrow().len(), 1, "one ask, however many frames");
    let ask = calls.borrow()[0].clone();
    assert_eq!(
        (ask.fg.as_str(), ask.bg.as_str(), ask.text),
        ("#c0caf5", "#1a1b26", SAMPLE_TEXT),
        "drawn on the card's own colour"
    );
    let ratio = scale(cx, view);
    assert_eq!(
        ask.sizes,
        physical(1., f64::from(ratio)),
        "the board's sizes in device pixels"
    );
    assert_eq!(
        physical(1., 1.),
        SAMPLE_SIZES,
        "which are the board's own at 1x"
    );
    assert!(
        !with(cx, view, |v| v.drawn()),
        "nothing draws before the answer"
    );
    answer(cx, view, ask.id, samples(&root, "a"), None);
    let (drawn, decoded) = with(cx, view, |v| (v.drawn(), v.decoded));
    assert_eq!(
        (drawn, decoded),
        (true, 1),
        "the three samples land in one decode"
    );
    let i18n = catalog();
    assert_eq!(with(cx, view, |v| v.status(&i18n)), "font");
    assert_eq!(
        with(cx, view, |v| v.facts_text().map(String::from)).as_deref(),
        Some("Inter · Bold · 312.0 kB · 2,548 glyphs")
    );
    let rects = with(cx, view, |v| v.rects((600., 500.), ratio, 16.))
        .expect("the samples are placed")
        .0;
    assert_eq!(rects.len(), 3, "three samples");
    assert!(
        rects.iter().all(|rect| rect.0 == 40.),
        "each starts at the 40 side inset"
    );
    assert_eq!(
        rects[1].1 - (rects[0].1 + rects[0].3),
        18.,
        "with the board's 18 gap"
    );
    let bar = with(cx, view, |v| v.bar(&i18n));
    assert_eq!(
        (bar.name.as_str(), bar.count, bar.views.is_none()),
        ("Inter-Bold.ttf", None, true),
        "the font bar has no count and no view words"
    );
}

#[gpui::test]
fn pv2_font_theme_change_keeps_the_old_samples_until_the_new_land(cx: &mut TestAppContext) {
    let root = root();
    let (view, calls) = open(cx);
    let first = calls.borrow()[0].id;
    answer(cx, view, first, samples(&root, "a"), None);
    view.update(cx, |v, _, cx| v.set_theme(theme("#343b58"), cx))
        .unwrap();
    draw(cx, view);
    assert_eq!(
        calls.borrow().len(),
        2,
        "a theme load asks once with both colours new"
    );
    assert!(with(cx, view, |v| v.drawn()), "the old samples stay up");
    answer(cx, view, first, samples(&root, "stale"), None);
    assert_eq!(
        with(cx, view, |v| v.decoded),
        1,
        "the old ask's answer is dropped"
    );
    let second = calls.borrow()[1].id;
    answer(cx, view, second, samples(&root, "b"), None);
    let shown = with(cx, view, |v| v.shown.as_ref().map(|s| s.images.len()));
    assert_eq!(
        (with(cx, view, |v| v.decoded), shown),
        (2, Some(3)),
        "the new three swap in together"
    );
}

#[gpui::test]
fn pv2_font_refusals_draw_the_unsupported_sentence(cx: &mut TestAppContext) {
    let root = root();
    let i18n = catalog();
    for (case, files, err) in [
        ("an error answer", Vec::new(), Some("not a font")),
        (
            "a short answer",
            samples(&root, "short")[..2].to_vec(),
            None,
        ),
    ] {
        let (view, calls) = open(cx);
        let id = calls.borrow()[0].id;
        answer(cx, view, id, files, err);
        let got = with(cx, view, |v| {
            (
                v.refusal(),
                v.status(&i18n),
                v.fail_sentence(&i18n),
                v.drawn(),
            )
        });
        let sentence = "This file cannot be previewed.";
        assert_eq!(
            got,
            (
                Some(Refusal::FontUnsupported),
                sentence.into(),
                Some(sentence.into()),
                false
            ),
            "{case}"
        );
    }
    let (view, calls) = open(cx);
    let id = calls.borrow()[0].id;
    let broken = root.join("broken.png");
    std::fs::write(&broken, b"not a png").unwrap();
    let path = broken.to_str().unwrap().to_owned();
    answer(cx, view, id, vec![path.clone(), path.clone(), path], None);
    assert_eq!(
        with(cx, view, |v| v.refusal()),
        Some(Refusal::FontUnsupported),
        "undecodable samples are a refusal, never half drawn"
    );
}

#[gpui::test]
fn pv2_font_facts_line_is_elided_to_the_page(cx: &mut TestAppContext) {
    let root = root();
    let (view, calls) = open(cx);
    let id = calls.borrow()[0].id;
    let reply = Reply {
        id,
        err: None,
        family: "Family ".repeat(80),
        style: "Bold".into(),
        glyphs: 2548,
        files: samples(&root, "long"),
    };
    view.update(cx, |v, _, cx| v.answered(reply, cx)).unwrap();
    draw(cx, view);
    let width = cx
        .update_window(view.into(), |_, window, _| {
            f32::from(window.viewport_size().width)
        })
        .unwrap();
    let (text, drawn) = with(cx, view, |v| {
        (v.facts_text().map(String::from), v.facts_width())
    });
    let (text, drawn) = (text.unwrap(), drawn.unwrap());
    assert!(
        text.ends_with('\u{2026}'),
        "the facts line is elided at its end: {text}"
    );
    assert!(
        drawn <= width - 80.,
        "and fits the page between its 40 px sides: {drawn} of {width}"
    );
}
