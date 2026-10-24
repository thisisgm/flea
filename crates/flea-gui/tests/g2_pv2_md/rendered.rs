//! Rendered Markdown: the first frame holds the first table, figure and picture, and the work per frame stays bounded by the frame.
use super::harness::{catalog, draw, first_screen, open, show, until, with, Helper};
use super::{file, root};
use flea_gui::g2::pv2::markdown::{Effect, View, FIGURE_HOLD_MS, MAX_BYTES};
use gpui::TestAppContext;
use std::time::Duration;

const LOGO: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="32" viewBox="0 0 64 32"><rect width="64" height="32" fill="#4a8"/></svg>"##;

const REPORT: &str = "# Report\n\n| Name | Size |\n|------|-----:|\n| a.txt | 12 |\n| b.txt | 340 |\n\n```mermaid\ngraph TD; A-->B\n```\n\n![logo](logo.svg)\n\nClosing words.\n";

#[gpui::test]
fn md_first_frame_holds_the_first_table_figure_and_picture(cx: &mut TestAppContext) {
    let root = root();
    file(&root, "logo.svg", LOGO.as_bytes());
    let path = file(&root, "report.md", REPORT.as_bytes());
    let view = open(cx, &root, Helper::Draws);
    draw(cx, view);
    first_screen(cx, view, &path);
    let counts = with(cx, view, |v| v.counts.clone());
    let first = counts.first.clone().unwrap();
    assert_eq!(first.tables, 1, "the first frame holds the table");
    assert_eq!(first.figures, 1, "and the diagram at its size");
    assert_eq!(first.pictures, 1, "and the picture");
    assert!(
        first.texts >= 8,
        "and the heading, cells and closing words: {first:?}"
    );
    assert_eq!(counts.parses, 1, "the document parses once");
    assert_eq!(
        counts.parsed_bytes,
        REPORT.len(),
        "the parse read the whole file"
    );
    assert_eq!(counts.ui_parse_bytes, 0, "and none of it on the UI thread");
    draw(cx, view);
    assert_eq!(
        with(cx, view, |v| v.counts.shaped_frame),
        0,
        "a frame that changes nothing shapes nothing"
    );
}

#[gpui::test]
fn md_a_failed_figure_draws_its_source_on_the_code_ground(cx: &mut TestAppContext) {
    let root = root();
    let path = file(&root, "fails.md", b"```mermaid\ngraph TD; A-->B\n```\n");
    let view = open(cx, &root, Helper::Fails);
    draw(cx, view);
    first_screen(cx, view, &path);
    let first = with(cx, view, |v| v.counts.first.clone().unwrap());
    assert_eq!(first.figures, 0, "a failed figure draws no picture");
    assert!(
        first.texts >= 1,
        "its fenced source is drawn instead: {first:?}"
    );
}

#[gpui::test]
fn md_a_figure_that_never_answers_holds_the_first_screen_only_until_the_cap(
    cx: &mut TestAppContext,
) {
    let root = root();
    // A helper that never answers leaves the figure asked; the clock moves only when the test moves it.
    let path = file(
        &root,
        "slow.md",
        b"Intro\n\n```mermaid\ngraph TD; A-->B\n```\n",
    );
    let view = open(cx, &root, Helper::Silent);
    draw(cx, view);
    show(cx, view, &path);
    until(
        cx,
        view,
        "the blocks are laid",
        |_| {},
        |v| v.counts.blocks_laid >= 2,
    );
    assert!(with(cx, view, |v| v.counts.first.is_none()
        || v.counts.first.as_ref().unwrap().figures == 0));
    cx.executor()
        .advance_clock(Duration::from_millis(FIGURE_HOLD_MS));
    until(
        cx,
        view,
        "the cap ends the hold",
        |_| {},
        |v| v.counts.first.is_some(),
    );
}

#[gpui::test]
fn md_layout_and_shaping_stay_bounded_by_the_frame_in_a_long_document(cx: &mut TestAppContext) {
    let root = root();
    let mut text = String::new();
    for i in 0..4000 {
        text.push_str(&format!(
            "Paragraph {i} holds a sentence long enough to wrap once in a narrow frame.\n\n"
        ));
    }
    let path = file(&root, "long.md", text.as_bytes());
    let view = open(cx, &root, Helper::Draws);
    draw(cx, view);
    first_screen(cx, view, &path);
    let (counts, height) = with(cx, view, |v| (v.counts.clone(), v.frame.1));
    // The frame and its cache on either side, at no less than one line per block.
    let bound = ((height + 2. * 600.) / 23.).ceil() as usize + 2;
    assert!(
        counts.first_job <= bound,
        "the first job lays out only the frame and its cache: {} of 4000, bound {bound}",
        counts.first_job
    );
    assert!(
        counts.largest_job <= bound.max(64),
        "every later job is a bounded chunk: {}",
        counts.largest_job
    );
    let first = counts.first.unwrap();
    assert!(
        first.texts < 200,
        "the first frame draws only what it shows: {first:?}"
    );
    assert!(
        counts.shaped_max < 400,
        "and shapes only that: {}",
        counts.shaped_max
    );
    assert_eq!(counts.ui_parse_bytes, 0);
    let before = with(cx, view, |v| v.counts.items_max);
    view.update(cx, |v, _, cx| {
        for _ in 0..40 {
            v.action("cursorDown", cx);
        }
    })
    .unwrap();
    draw(cx, view);
    let (scroll, items, shaped) = with(cx, view, |v| {
        (v.scroll, v.counts.items_frame, v.counts.shaped_frame)
    });
    assert!(scroll > 0., "j scrolls the document");
    assert!(
        items <= before.max(1) * 2,
        "a scrolled frame draws as little as the first: {items}"
    );
    assert!(
        shaped < 400,
        "and shapes only the lines it newly shows: {shaped}"
    );
}

#[gpui::test]
fn md_bar_counts_lines_and_keys_flip_the_view_and_walk_to_close(cx: &mut TestAppContext) {
    let root = root();
    let path = file(
        &root,
        "notes.md",
        b"# Notes\r\n\r\nOne line.\r\nTwo lines.\r\n",
    );
    let view = open(cx, &root, Helper::Draws);
    draw(cx, view);
    first_screen(cx, view, &path);
    let i18n = catalog(&root);
    let bar = with(cx, view, |v| v.bar(&i18n));
    assert_eq!(bar.name, "notes.md");
    assert_eq!(bar.count.as_deref(), Some("4 lines"));
    assert_eq!(bar.views[0].1, "Rendered");
    assert!(
        bar.views[0].2 && !bar.views[1].2,
        "Rendered is the view in effect"
    );
    assert_eq!(bar.close, "Close");
    let effects = view
        .update(cx, |v, _, cx| {
            let flip = v.action("markdownView", cx);
            let walk = v.action("focusNext", cx);
            let close = v.action("open", cx);
            (flip, walk, close, v.shown_view(), v.close_focused)
        })
        .unwrap();
    assert_eq!(
        effects,
        (
            Effect::View(View::Source),
            Effect::CloseFocus(true),
            Effect::Close,
            View::Source,
            true
        )
    );
    assert!(
        with(cx, view, |v| v.bar(&i18n).views[1].2),
        "the bar marks Source"
    );
}

#[gpui::test]
fn md_only_a_link_with_an_allowed_scheme_opens(cx: &mut TestAppContext) {
    let root = root();
    let path = file(
        &root,
        "links.md",
        b"[site](https://example.com/a)\n\n[local](notes.md)\n\n[script](javascript:alert(1))\n",
    );
    let view = open(cx, &root, Helper::Draws);
    draw(cx, view);
    first_screen(cx, view, &path);
    let links = with(cx, view, |v| v.links());
    let at = |url: &str| {
        let (rect, _) = links
            .iter()
            .find(|(_, u)| u == url)
            .unwrap_or_else(|| panic!("{url} drawn as a link: {links:?}"));
        (rect.0 + 1., rect.1 + 1.)
    };
    let opened = view
        .update(cx, |v, _, cx| {
            v.open_link_at(at("https://example.com/a"), cx)
        })
        .unwrap();
    assert!(opened);
    assert_eq!(cx.opened_url().as_deref(), Some("https://example.com/a"));
    if let Some((rect, _)) = links.iter().find(|(_, u)| u.contains("notes.md")) {
        let point = (rect.0 + 1., rect.1 + 1.);
        assert!(
            !view
                .update(cx, |v, _, cx| v.open_link_at(point, cx))
                .unwrap(),
            "a local link opens nothing"
        );
    }
    assert!(
        links.iter().all(|(_, u)| !u.starts_with("javascript")),
        "a script link is never a link: {links:?}"
    );
    assert_eq!(cx.opened_url().as_deref(), Some("https://example.com/a"));
}

#[gpui::test]
fn md_a_file_too_large_or_unreadable_says_so_in_either_view(cx: &mut TestAppContext) {
    let root = root();
    let big = file(&root, "big.md", &vec![b'a'; MAX_BYTES as usize + 1]);
    std::fs::create_dir(root.join("folder.md")).unwrap();
    let view = open(cx, &root, Helper::Draws);
    draw(cx, view);
    show(cx, view, &big);
    until(
        cx,
        view,
        "the refusal",
        |_| {},
        |v| v.status_sentence().is_some(),
    );
    assert_eq!(
        with(cx, view, |v| v.status_sentence().map(str::to_string)).as_deref(),
        Some("This file is too large to preview.")
    );
    assert_eq!(
        with(cx, view, |v| v.counts.parses),
        0,
        "a refused file is never parsed"
    );
    view.update(cx, |v, _, cx| v.set_view(View::Source, cx))
        .unwrap();
    draw(cx, view);
    assert_eq!(
        with(cx, view, |v| v.source_refused()),
        Some(flea_gui::g2::pv2::markdown::Answer::Large)
    );
    show(cx, view, &root.join("folder.md"));
    until(
        cx,
        view,
        "the unreadable sentence",
        |_| {},
        |v| v.status_sentence() == Some("This file could not be read."),
    );
}
