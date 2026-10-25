//! A damaged page, held turns, a dropped view, a long document's open and a zoomed neighbour, each against a real render.
use super::fixture::{self, LETTER};
use super::view::finish;
use super::view::{draw, next_frame, open, palette, show, until, with, METRICS};
use super::{document, reaper_lock, root};
use flea::ui_logic_pv2::viewer;
use flea_gui::g2::pv2::reap;
use flea_gui::g2::pv2::PdfView;
use gpui::{div, prelude::*, Context, Entity, TestAppContext, Window};
use std::time::{Duration, Instant};

#[gpui::test]
fn pv2_a_damaged_page_is_asked_for_once(cx: &mut TestAppContext) {
    let _reaper = reaper_lock();
    let root = root();
    let path = root.join("damaged.pdf");
    let bytes = String::from_utf8(fixture::pdf(&[LETTER]))
        .unwrap()
        .replace("/Count 1", "/Count 3");
    std::fs::write(&path, bytes).unwrap();
    let view = open(cx, false);
    draw(cx, view);
    show(cx, view, &path);
    next_frame(cx, view);
    until(cx, view, "page 1 lands", |v| {
        v.surface.shown_page() == Some(0)
    });
    let rest = Instant::now() + Duration::from_millis(300);
    while Instant::now() < rest {
        draw(cx, view);
        std::thread::sleep(Duration::from_millis(2));
    }
    let (pages, asked, failed) = with(cx, view, |v| {
        (
            v.surface.sizes.len(),
            v.surface.counters.asked,
            v.surface.counters.failed,
        )
    });
    assert_eq!(pages, 3, "the damaged count claims three pages");
    assert_eq!(failed, 1, "at rest the one damaged neighbour failed once");
    assert!(asked <= 3, "at rest the view asked {asked} renders");
    finish(cx, view);
}

#[gpui::test]
fn pv2_held_turns_land_the_last_page_without_rendering_every_page_passed(cx: &mut TestAppContext) {
    let _reaper = reaper_lock();
    let root = root();
    let path = document(&root, "long.pdf", &[LETTER; 400]);
    let view = open(cx, false);
    draw(cx, view);
    show(cx, view, &path);
    next_frame(cx, view);
    until(cx, view, "page 1 lands", |v| {
        v.surface.shown_page() == Some(0)
    });
    view.update(cx, |v, _, cx| {
        for _ in 0..399 {
            v.turn(1, cx);
        }
    })
    .unwrap();
    until(cx, view, "page 400 lands", |v| {
        v.surface.shown_page() == Some(399) && v.surface.inflight.is_empty()
    });
    let landed = with(cx, view, |v| v.surface.counters.landed);
    assert!(
        landed <= 40,
        "399 held turns landed {landed} renders, not one per page passed"
    );
    finish(cx, view);
}

struct Host {
    child: Option<Entity<PdfView>>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().children(self.child.clone())
    }
}

#[gpui::test]
fn pv2_a_dropped_view_releases_its_pages_from_the_atlas(cx: &mut TestAppContext) {
    let _reaper = reaper_lock();
    let root = root();
    let path = document(&root, "pages.pdf", &[LETTER; 3]);
    cx.executor().allow_parking();
    let host = cx.add_window(|_, cx| Host {
        child: Some(cx.new(|cx| PdfView::new(false, palette(false), METRICS, cx))),
    });
    let child = host.update(cx, |h, _, _| h.child.clone().unwrap()).unwrap();
    let path = path.to_str().unwrap().to_owned();
    host.update(cx, |_, window, cx| {
        child.update(cx, |v, cx| {
            v.set_active(true, window, cx);
            v.set_path(&path, window, cx);
        })
    })
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    let image = loop {
        cx.update_window(host.into(), |_, window, cx| window.simulate_next_frame(cx))
            .unwrap();
        cx.run_until_parked();
        let image = child.read_with(cx, |v, _| v.surface.single_image().cloned());
        if let Some(image) = image {
            break image;
        }
        assert!(Instant::now() < deadline, "page 1 lands within 20 s");
        std::thread::sleep(Duration::from_millis(2));
    };
    cx.update_window(host.into(), |_, window, _| window.refresh())
        .unwrap();
    cx.run_until_parked();
    let held = |cx: &mut TestAppContext| {
        cx.update_window(host.into(), |_, window, _| {
            window.has_image_atlas_entry(&image)
        })
        .unwrap()
    };
    assert!(held(cx), "the shown page is in the window's atlas");
    drop(child);
    host.update(cx, |h, _, cx| {
        h.child = None;
        cx.notify();
    })
    .unwrap();
    for _ in 0..2 {
        cx.update_window(host.into(), |_, window, _| window.refresh())
            .unwrap();
        cx.run_until_parked();
    }
    assert!(!held(cx), "a dropped view leaves no page in the atlas");
    reap();
}

// The order of the reads is the renderer's, pinned by renderer.rs; the view lands page 1 and counts each size once.
#[gpui::test]
fn pv2_quick_look_lands_its_first_page_and_counts_every_size_once(cx: &mut TestAppContext) {
    let _reaper = reaper_lock();
    let root = root();
    let path = document(&root, "long.pdf", &[LETTER; 2000]);
    let view = open(cx, false);
    draw(cx, view);
    show(cx, view, &path);
    next_frame(cx, view);
    until(cx, view, "page 1 lands", |v| {
        v.surface.shown_page() == Some(0)
    });
    until(cx, view, "every size is counted", |v| {
        v.surface.counters.size_reads >= 2000 && v.surface.inflight.is_empty()
    });
    let reads = with(cx, view, |v| v.surface.counters.size_reads);
    assert_eq!(reads, 2000, "each of the 2000 sizes is read once");
    finish(cx, view);
}

#[gpui::test]
fn pv2_a_zoomed_page_prefetches_its_neighbours_fitted(cx: &mut TestAppContext) {
    let _reaper = reaper_lock();
    let root = root();
    let path = document(&root, "pages.pdf", &[LETTER; 3]);
    let view = open(cx, false);
    draw(cx, view);
    show(cx, view, &path);
    next_frame(cx, view);
    until(cx, view, "page 1 and its neighbour land", |v| {
        v.surface.shown_page() == Some(0) && v.surface.inflight.is_empty()
    });
    let width = |v: &PdfView, page: usize| {
        v.surface
            .cache
            .nearest(page, 0)
            .map(|image| image.size(0).width.0)
    };
    let fitted = with(cx, view, |v| width(v, 1)).expect("page 2 is prefetched");
    for _ in 0..8 {
        view.update(cx, |v, _, cx| v.action(viewer::Action::ZoomIn, cx))
            .unwrap();
    }
    until(cx, view, "the zoomed page lands", |v| {
        v.surface.inflight.is_empty() && width(v, 0) > Some(fitted)
    });
    assert_eq!(
        with(cx, view, |v| width(v, 1)),
        Some(fitted),
        "the neighbour stays at its fitted width under zoom"
    );
    finish(cx, view);
}
