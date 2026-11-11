//! The PDF view as a real GPUI entity in a test window, rendering real documents through its own thread.
use super::fixture::{Page, LANDSCAPE, LETTER, TURNED};
use super::{document, reaper_lock, root};
use flea::ui_logic_doc_preview::{pdf, viewer};
use flea_gui::g2::doc_preview::render::{reap, reaper_len};
use flea_gui::g2::doc_preview::{Metrics, Palette, PdfView};
use gpui::{AppContext as _, TestAppContext, WindowHandle};
use std::time::{Duration, Instant};

pub(super) const METRICS: Metrics = Metrics {
    gap: 9.,
    margin: 28.,
    row_height: 28.,
};

pub(super) fn palette(light: bool) -> Palette {
    let theme = flea_gui::palette::Theme::default();
    Palette {
        background: if light {
            theme.foreground
        } else {
            theme.background
        },
    }
}

// The render thread is real, so its wakes come from outside the deterministic scheduler, the case parking exists for.
pub(super) fn open(cx: &mut TestAppContext, continuous: bool) -> WindowHandle<PdfView> {
    cx.executor().allow_parking();
    cx.add_window(|_, cx| PdfView::new(continuous, palette(false), METRICS, cx))
}

pub(super) fn with<R>(
    cx: &mut TestAppContext,
    view: WindowHandle<PdfView>,
    f: impl FnOnce(&mut PdfView) -> R,
) -> R {
    view.update(cx, |view, _, _| f(view)).unwrap()
}

pub(super) fn draw(cx: &mut TestAppContext, view: WindowHandle<PdfView>) {
    cx.update_window(view.into(), |_, window, _| window.refresh())
        .unwrap();
    cx.run_until_parked();
}

pub(super) fn next_frame(cx: &mut TestAppContext, view: WindowHandle<PdfView>) -> usize {
    cx.update_window(view.into(), |_, window, cx| window.simulate_next_frame(cx))
        .unwrap()
}

// Real renders land from another thread, so the test runs the foreground until the condition holds.
pub(super) fn until(
    cx: &mut TestAppContext,
    view: WindowHandle<PdfView>,
    what: &str,
    done: impl Fn(&PdfView) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        draw(cx, view);
        let (finished, failed) = with(cx, view, |v| (done(v), v.surface.failed()));
        assert!(!failed, "{what}: the surface failed instead");
        if finished {
            return;
        }
        assert!(Instant::now() < deadline, "{what} within 20 s");
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Closes the window so its view and renderer drop, then reaps the renderer thread as main does before it returns.
pub(super) fn finish(cx: &mut TestAppContext, view: WindowHandle<PdfView>) {
    cx.update_window(view.into(), |_, window, _| window.remove_window())
        .unwrap();
    cx.run_until_parked();
    assert!(
        reaper_len() >= 1,
        "the closed window's renderer waits in the reaper"
    );
    reap();
    assert_eq!(reaper_len(), 0, "the reap joined every dropped renderer");
}

pub(super) fn show(cx: &mut TestAppContext, view: WindowHandle<PdfView>, path: &std::path::Path) {
    let path = path.to_str().unwrap().to_owned();
    view.update(cx, |v, window, cx| {
        v.set_active(true, window, cx);
        v.set_path(&path, window, cx);
    })
    .unwrap();
}

#[gpui::test]
fn pv2_column_stack_fills_the_frame_and_builds_pages_near_it(cx: &mut TestAppContext) {
    let _reaper = reaper_lock();
    let root = root();
    let path = document(&root, "report.pdf", &[LETTER; 12]);
    let view = open(cx, true);
    draw(cx, view);
    show(cx, view, &path);
    assert!(
        with(cx, view, |v| v.surface.opener.fast_armed),
        "an isolated press waits for the next frame"
    );
    assert_eq!(next_frame(cx, view), 1, "the press asked for one frame");
    assert_eq!(
        with(cx, view, |v| v.surface.opener.fast_opens),
        1,
        "and opened on it"
    );
    until(cx, view, "the first page lands", |v| {
        v.surface.shown_page().is_some()
    });
    let (width, scale, layout, size_reads, render_width) = with(cx, view, |v| {
        let s = &v.surface;
        (
            s.width,
            s.scale,
            s.layout.clone(),
            s.counters.size_reads,
            s.render_width,
        )
    });
    let page = (width * 792. / 612.).round();
    assert_eq!(
        layout.heights[0], page,
        "page 1 is as wide as the frame and keeps its proportion"
    );
    assert_eq!(
        layout.height,
        12. * page + 11. * f64::from(METRICS.gap),
        "the content is twelve pages and eleven gaps"
    );
    assert_eq!(size_reads, 12, "the page sizes are read once, at the open");
    assert_eq!(
        render_width,
        (width * scale).round() as u32,
        "the render is the frame's width in device pixels"
    );
    let (drawn, near, cached) = with(cx, view, |v| {
        (
            v.drawn_pages().map(|(p, _, _)| p).collect::<Vec<_>>(),
            v.surface.slots.len(),
            v.surface.cache.len(),
        )
    });
    assert_eq!(
        drawn.first(),
        Some(&0),
        "page 1 is drawn at the frame's top"
    );
    let in_view = layout
        .in_view(0., with(cx, view, |v| v.surface.view_height))
        .len();
    assert!(
        near <= in_view + 2 && cached <= near + 2,
        "at rest only the pages near the frame are built: {near} slots, {cached} images"
    );
    assert!(with(cx, view, |v| v.painted.get()) > 0, "the view painted");
    finish(cx, view);
}

#[gpui::test]
fn pv2_column_scroll_follows_the_offset_and_keeps_the_window_bounded(cx: &mut TestAppContext) {
    let _reaper = reaper_lock();
    let root = root();
    let path = document(&root, "big.pdf", &[LETTER; 500]);
    let view = open(cx, true);
    draw(cx, view);
    show(cx, view, &path);
    next_frame(cx, view);
    until(cx, view, "the first page lands", |v| {
        v.surface.shown_page().is_some()
    });
    view.update(cx, |v, _, cx| {
        for _ in 0..6 {
            v.turn(1, cx);
        }
    })
    .unwrap();
    until(cx, view, "the pages near page 7 land", |v| {
        v.drawn_pages().count() > 0 && v.surface.inflight.is_empty()
    });
    let (page, top, at, slots, images, reads) = with(cx, view, |v| {
        let s = &v.surface;
        (
            s.page,
            s.view_top,
            s.layout.page_at(s.view_top),
            s.slots.len(),
            s.cache.len(),
            s.counters.size_reads,
        )
    });
    assert_eq!(page, 6, "six turns reach page 7");
    assert_eq!(
        page, at,
        "the page under the frame's top is the one its offset names: top {top}"
    );
    assert!(
        slots <= 6 && images <= slots + 2,
        "a 500-page document builds only near the frame: {slots} slots, {images} images"
    );
    assert_eq!(reads, 500, "and reads its 500 page sizes once");
    finish(cx, view);
}

#[gpui::test]
fn pv2_quick_look_turns_from_its_cache_and_counts_the_drawn_page(cx: &mut TestAppContext) {
    let _reaper = reaper_lock();
    let root = root();
    let path = document(&root, "pages.pdf", &[LETTER, LANDSCAPE, TURNED]);
    let view = open(cx, false);
    draw(cx, view);
    let started = Instant::now();
    show(cx, view, &path);
    next_frame(cx, view);
    until(cx, view, "page 1 lands", |v| {
        v.surface.shown_page() == Some(0)
    });
    let first = started.elapsed();
    until(cx, view, "its neighbour lands", |v| {
        v.surface.inflight.is_empty()
    });
    let rect = with(cx, view, |v| v.single_rect()).expect("page 1 is drawn");
    let aspect = rect.2 / rect.3;
    assert!(
        (aspect - 612. / 792.).abs() < 0.01,
        "the page keeps the document's proportion: {aspect}"
    );
    assert!(
        with(cx, view, |v| v.surface.slots.is_empty()),
        "Quick Look keeps the single page, no stack"
    );
    let turned = Instant::now();
    view.update(cx, |v, _, cx| v.turn(1, cx)).unwrap();
    let turn = turned.elapsed();
    let render = with(cx, view, |v| v.surface.counters.last_render);
    let ms = |d: Duration| d.as_secs_f64() * 1e3;
    println!(
        "PV2_TIMING quick_look first_page_ms={:.1} prefetched_turn_ms={:.3} last_raster_ms={:.1}",
        ms(first),
        ms(turn),
        ms(render)
    );
    let (shown, fell_back, drawn) = with(cx, view, |v| {
        (
            v.surface.shown_page(),
            v.surface.cap.fell_back,
            v.surface.drawn_page(),
        )
    });
    assert_eq!(
        (shown, fell_back, drawn),
        (Some(1), false, 1),
        "a turn to a page already rendered shows it in the same turn"
    );
    let effect = view
        .update(cx, |v, _, cx| v.action(viewer::Action::ZoomIn, cx))
        .unwrap();
    assert_eq!(
        effect,
        viewer::Effect::Zoom(pdf::ZOOM_MIN + pdf::ZOOM_STEP),
        "zoom in steps up one rung"
    );
    view.update(cx, |v, window, cx| {
        v.set_path(path.to_str().unwrap(), window, cx)
    })
    .unwrap();
    assert_eq!(
        with(cx, view, |v| v.viewer.clone().unwrap().zoom),
        pdf::ZOOM_MIN,
        "the subject opens fitted again"
    );
    finish(cx, view);
}

#[gpui::test]
fn pv2_a_walk_waits_for_the_settle_and_a_switch_drops_the_old_pages(cx: &mut TestAppContext) {
    let _reaper = reaper_lock();
    let root = root();
    let a = document(&root, "a.pdf", &[LETTER]);
    let b = document(&root, "b.pdf", &[LANDSCAPE]);
    let c = document(&root, "c.pdf", &[TURNED]);
    let view = open(cx, false);
    draw(cx, view);
    show(cx, view, &a);
    next_frame(cx, view);
    until(cx, view, "a lands", |v| v.surface.shown_page() == Some(0));
    cx.executor()
        .advance_clock(Duration::from_millis(pdf::DOCUMENT_SETTLE_MS));
    show(cx, view, &c);
    show(cx, view, &b);
    assert!(
        !with(cx, view, |v| v.surface.opener.fast_armed),
        "the next step inside the settle drops the frame path"
    );
    next_frame(cx, view);
    assert_eq!(
        with(cx, view, |v| v.surface.opener.opened.clone()),
        a.to_str().unwrap(),
        "a frame opens nothing"
    );
    cx.executor()
        .advance_clock(Duration::from_millis(pdf::DOCUMENT_SETTLE_MS));
    cx.run_until_parked();
    assert_eq!(
        with(cx, view, |v| v.surface.opener.opened.clone()),
        b.to_str().unwrap(),
        "the walk lands at the settle"
    );
    assert_eq!(
        with(cx, view, |v| (v.surface.shown_page(), v.single_rect())),
        (None, None),
        "another document's page is never drawn"
    );
    until(cx, view, "b lands", |v| v.surface.shown_page() == Some(0));
    let rect = with(cx, view, |v| v.single_rect()).unwrap();
    assert!(rect.2 > rect.3, "the landed page is b's landscape page");
    finish(cx, view);
}

#[test]
fn pv2_the_veil_is_for_dark_grounds_only() {
    assert_eq!(
        palette(true).veil(),
        None,
        "a light theme leaves the page as authored"
    );
    let veil = palette(false).veil().expect("a dark theme veils the page");
    assert!((veil.a - 0.05).abs() < 1e-6, "at 5% of the window ground");
    let _ = Page {
        width: 1.,
        height: 1.,
        rotate: 0,
    };
}

#[gpui::test]
fn pv2_quick_look_strip_reads_the_catalog_and_a_failed_open_gets_one_sentence(
    cx: &mut TestAppContext,
) {
    let _reaper = reaper_lock();
    let i18n = crate::test_catalogs::english();
    let root = root();
    let view = open(cx, false);
    draw(cx, view);
    let bar = with(cx, view, |v| v.bar(&i18n)).expect("Quick Look has a strip");
    assert_eq!(bar.counter, None, "no count before the pages are known");
    let names: Vec<(&str, String)> = bar
        .items
        .iter()
        .map(|(_, glyph, name)| (*glyph, name.clone()))
        .collect();
    let want = [
        ("chevron-left", "Previous page"),
        ("chevron-right", "Next page"),
        ("minus", "Zoom out"),
        ("plus", "Zoom in"),
        ("maximize", "Expand"),
        ("x", "Close"),
    ];
    assert_eq!(
        names,
        want.map(|(g, n)| (g, n.to_string())).to_vec(),
        "PdfViewer.qml's glyphs and names, through the catalog"
    );
    assert_eq!(
        with(cx, view, |v| v.fail_sentence(&i18n)),
        None,
        "nothing failed yet"
    );
    show(cx, view, &root.join("missing.pdf"));
    next_frame(cx, view);
    let deadline = Instant::now() + Duration::from_secs(20);
    while !with(cx, view, |v| v.surface.failed()) {
        assert!(Instant::now() < deadline, "the open fails within 20 s");
        draw(cx, view);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(
        with(cx, view, |v| v.fail_sentence(&i18n)).as_deref(),
        Some("This file could not be read."),
        "a document that cannot be read gets the oracle's sentence"
    );
    assert_eq!(
        with(cx, view, |v| (v.surface.shown_page(), v.single_rect())),
        (None, None),
        "and no page is drawn"
    );
    let column = open(cx, true);
    draw(cx, column);
    assert!(
        with(cx, column, |v| v.bar(&i18n)).is_none(),
        "the column has no strip"
    );
    finish(cx, view);
    finish(cx, column);
}
