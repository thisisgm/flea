use super::spinner::{device_side, mapped, Crawl, Mode, Motion};
use super::spinner_tests::{frame, FRAME};
use crate::{
    list::{bench::allocations, Palette},
    palette::Theme,
};
use gpui::{
    div, prelude::*, px, size, App, Bounds, Context, Element, ElementId, GlobalElementId, Hsla,
    InspectorElementId, LayoutId, Pixels, Render, Style, TestAppContext, Window,
};
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};

struct Probe {
    mark: Rc<Crawl>,
    pinned: Option<f32>,
    side: f32,
    spent: Rc<Cell<u64>>,
    samples_and_paints: (usize, usize),
}
impl Render for Probe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().p(px(30.)).child(Measured {
            mark: Rc::clone(&self.mark),
            pinned: self.pinned,
            side: self.side,
            spent: Rc::clone(&self.spent),
            samples_and_paints: self.samples_and_paints,
        })
    }
}
struct Measured {
    mark: Rc<Crawl>,
    pinned: Option<f32>,
    side: f32,
    spent: Rc<Cell<u64>>,
    samples_and_paints: (usize, usize),
}
impl IntoElement for Measured {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for Measured {
    type RequestLayoutState = ();
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (
            window.request_layout(
                Style {
                    size: size(px(self.side).into(), px(self.side).into()),
                    ..Default::default()
                },
                None,
                cx,
            ),
            (),
        )
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.mark.place(bounds, window);
        for sample in 0..self.samples_and_paints.0 {
            let pinned = self.pinned.map(|seconds| seconds + sample as f32 / 10.);
            self.mark.prepare(true, pinned, window, cx);
        }
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        _: &mut App,
    ) {
        let before = allocations();
        for _ in 0..self.samples_and_paints.1 {
            self.mark.paint(window);
        }
        self.spent.set(allocations() - before);
    }
}

#[test]
fn the_slot_uses_whole_22_device_pixel_steps() {
    for (slot, want) in [
        (16., 22.),
        (22., 22.),
        (30., 22.),
        (44., 44.),
        (48., 44.),
        (66., 66.),
        (82.5, 66.),
        (99., 88.),
        (132., 132.),
        // Edges snapped at a fractional scale read a whole-pixel slot back a hair under, 87.99999 at 1.25.
        (43.99999, 44.),
        (87.99999, 88.),
        (131.99998, 132.),
    ] {
        assert_eq!(device_side(slot), want);
    }
}

#[gpui::test]
fn every_painted_quad_matches_upstream_at_all_scales_sizes_modes_and_themes(
    cx: &mut TestAppContext,
) {
    for scale in [1., 1.25, 1.5, 2.] {
        for side in [16., 22., 30., 44., 48., 66.] {
            for mode in [Mode::Thinking, Mode::Working, Mode::Upload, Mode::Download] {
                for text in [
                    "background='#101315'\nforeground='#cacccc'",
                    "background='#f2f2f2'\nforeground='#404040'",
                ] {
                    let theme = Theme::from_text(text);
                    let mark = Rc::new(Crawl::default());
                    let held = Rc::clone(&mark);
                    let handle = cx.add_window(move |window, _| {
                        held.set(mode);
                        held.show(true, Palette::inherited(&window.text_style(), theme).accent);
                        Probe {
                            mark: held,
                            pinned: Some(0.713),
                            side: side / scale,
                            spent: Rc::default(),
                            samples_and_paints: (1, 1),
                        }
                    });
                    cx.simulate_window_scale_factor_change(handle.into(), scale);
                    handle
                        .update(cx, |_, window, _| {
                            let mut expected = omi::Player::with_options(
                                &omi::Kit::bundled(),
                                omi::Options {
                                    mode: Some(mode.id().into()),
                                    body_motion: device_side(side) >= 44.,
                                    ..Default::default()
                                },
                            );
                            expected.frame(0.);
                            let rects = expected.mode_rects(mode.id(), 0.713).unwrap();
                            let mut quads = window.painted_quads();

                            for rect in &rects {
                                let bounds = reference_bounds(rect, expected.kit().view, mark.slot(), scale, side);
                                if bounds.is_empty() { continue; }
                                let ink = mark.ink().opacity(rect.opacity as f32);
                                let index = quads.iter().position(|quad|
                                    quad.bounds == bounds && quad.background.as_solid() == Some(ink));
                                let index = index.unwrap_or_else(|| panic!("missing {mode:?} quad {bounds:?} at scale {scale}, side {side}"));
                                let quad = quads.remove(index);
                                assert_eq!(quad.bounds, bounds);
                                for edge in [bounds.origin.x.0, bounds.origin.y.0, bounds.right().0, bounds.bottom().0] {
                                    assert!((edge - edge.round()).abs() < 0.0001);
                                }
                                assert_eq!(quad.background.as_solid(), Some(ink));
                            }
                            assert!(quads.is_empty(), "extra {mode:?} quads at scale {scale}, side {side}");
                        })
                        .unwrap();
                }
            }
        }
    }
}

#[test]
fn rest_and_small_body_motion_match_the_pinned_crate() {
    let now = Instant::now();
    for mode in [Mode::Thinking, Mode::Working, Mode::Upload, Mode::Download] {
        for side in [22., 44., 66.] {
            let mut motion = Motion::new(mode);
            motion.sample(false, None, side, now);
            let player = omi::Player::with_options(
                &omi::Kit::bundled(),
                omi::Options {
                    body_motion: side >= 44.,
                    ..Default::default()
                },
            );
            assert_eq!(motion.rects(), player.mode_rects(mode.id(), 0.).unwrap());
            motion.sample(true, Some(0.713), side, now + Duration::from_secs(1));
            assert_eq!(motion.rects(), player.mode_rects(mode.id(), 0.713).unwrap());
        }
    }
}

#[test]
fn a_mid_loop_mode_change_morphs_without_a_blank_frame_and_settles() {
    let now = Instant::now();
    let mut motion = Motion::new(Mode::Thinking);
    for tick in 0..45 {
        motion.sample(true, None, 44., now + FRAME * tick);
    }
    let before = motion.rects().to_vec();
    motion.set(Mode::Working);
    assert!(!motion.settled());
    motion.sample(true, None, 44., now + FRAME * 44);
    assert_eq!(
        raster(motion.rects(), motion.view(), 44),
        raster(&before, motion.view(), 44)
    );
    for tick in 45..110 {
        motion.sample(true, None, 44., now + FRAME * tick);
        assert!(!motion.rects().is_empty());
    }
    assert!(motion.settled());
    motion.set(Mode::Thinking);
    for tick in 110..185 {
        motion.sample(true, None, 44., now + FRAME * tick);
        assert!(!motion.rects().is_empty());
        if tick == 116 {
            motion.set(Mode::Upload);
        }
    }
    assert!(motion.settled());
}

#[gpui::test]
fn painting_prepared_omi_rects_allocates_nothing(cx: &mut TestAppContext) {
    let mark = Rc::new(Crawl::default());
    let spent = Rc::new(Cell::new(0));
    let (held, measured) = (Rc::clone(&mark), Rc::clone(&spent));
    let handle = cx.add_window(move |_, _| {
        held.show(true, Hsla::default());
        Probe {
            mark: held,
            pinned: Some(0.713),
            side: 44.,
            spent: measured,
            samples_and_paints: (1, 1),
        }
    });
    for _ in 0..64 {
        handle.update(cx, |_, window, _| window.refresh()).unwrap();
        frame(cx, handle);
    }
    assert_eq!(spent.get(), 0);
    assert_eq!(mark.counters.frames.get(), mark.counters.writes.get());
    assert_eq!(mark.counters.requests.get(), 0);
    eprintln!(
        "SPINNER_ALLOCS prepared_paint={} frames={}",
        spent.get(),
        mark.counters.frames.get()
    );
}

#[test]
fn pinned_stills_are_exported_from_the_crate_without_copying_the_kit() {
    let Some(root) = std::env::var_os("FLEA_SPINNER_SPECIMENS") else {
        return;
    };
    for mode in [Mode::Thinking, Mode::Working, Mode::Upload, Mode::Download] {
        for side in [22, 44, 66] {
            let mut motion = Motion::new(mode);
            motion.sample(false, None, side as f32, Instant::now());
            let pixels = raster(motion.rects(), motion.view(), side);
            let mut bytes = format!("P5\n{side} {side}\n255\n").into_bytes();
            bytes.extend(pixels);
            std::fs::write(
                std::path::Path::new(&root).join(format!("spinner-{}-{side}.pgm", mode.id())),
                bytes,
            )
            .unwrap();
            eprintln!("SPINNER_BOARD {} {side}", mode.id());
        }
    }
}

fn raster(rects: &[omi::Rect], view: [f64; 4], side: usize) -> Vec<u8> {
    let mut pixels = vec![255.; side * side];
    let slot = Bounds::new(
        gpui::point(px(0.), px(0.)),
        size(px(side as f32), px(side as f32)),
    );
    for rect in rects {
        let bounds = mapped(rect, view, slot, 1.);
        for y in f32::from(bounds.origin.y).max(0.) as usize
            ..(f32::from(bounds.bottom()) as usize).min(side)
        {
            for x in f32::from(bounds.origin.x).max(0.) as usize
                ..(f32::from(bounds.right()) as usize).min(side)
            {
                pixels[y * side + x] *= 1. - rect.opacity;
            }
        }
    }
    pixels
        .into_iter()
        .map(|value| value.round() as u8)
        .collect()
}

fn reference_bounds(
    rect: &omi::Rect,
    view: [f64; 4],
    slot: Bounds<Pixels>,
    scale: f32,
    side: f32,
) -> Bounds<gpui::ScaledPixels> {
    let size = device_side(side);
    let left = (f32::from(slot.origin.x) * scale + (side - size) / 2.).round();
    let top = (f32::from(slot.origin.y) * scale + (side - size) / 2.).round();
    let unit = f64::from(size) / view[2];
    let edges = [
        (f64::from(left) + (rect.x - view[0]) * unit).round() as f32,
        (f64::from(top) + (rect.y - view[1]) * unit).round() as f32,
        (f64::from(left) + (rect.x + rect.w - view[0]) * unit).round() as f32,
        (f64::from(top) + (rect.y + rect.h - view[1]) * unit).round() as f32,
    ];
    Bounds::from_corners(
        gpui::point(px(edges[0]), px(edges[1])),
        gpui::point(px(edges[2]), px(edges[3])),
    )
    .scale(1.)
}

#[gpui::test]
fn independent_counters_detect_extra_samples_and_extra_paints(cx: &mut TestAppContext) {
    for (samples, paints) in [(1, 1), (2, 1), (1, 2)] {
        let mark = Rc::new(Crawl::default());
        let held = Rc::clone(&mark);
        let handle = cx.add_window(move |_, _| {
            held.show(true, Hsla::default());
            Probe {
                mark: held,
                pinned: Some(0.1),
                side: 44.,
                spent: Rc::default(),
                samples_and_paints: (samples, paints),
            }
        });
        handle.update(cx, |_, _, _| {
            let writes = mark.counters.writes.get();
            let frames = mark.counters.frames.get();
            assert_eq!((writes, frames), (samples, paints));
            assert_eq!(writes == frames, samples == paints);
            assert_eq!(mark.offset(), 0.1 + (samples - 1) as f32 / 10.);
            assert_eq!(mark.counters.requests.get(), 0);
            eprintln!("SPINNER_COUNTER_CONTROL samples={samples} paints={paints} writes={writes} frames={frames}");
        }).unwrap();
    }
}

/// An isolated host paints only the mark's prepared sample, without sampling or quad selection.
pub(super) fn capture_rest(cx: &mut TestAppContext, mark: Rc<Crawl>) -> (Vec<gpui::Quad>, f32) {
    let handle = cx.add_window(move |_, _| Probe {
        mark,
        pinned: None,
        side: 44.,
        spent: Rc::default(),
        samples_and_paints: (0, 1),
    });
    handle
        .update(cx, |_, window, _| {
            (window.painted_quads(), window.scale_factor())
        })
        .unwrap()
}
