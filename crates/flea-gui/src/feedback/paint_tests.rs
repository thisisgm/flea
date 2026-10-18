use super::spinner::{cells, phase, Crawl, Slot, GRID, PERIOD_MS, TRAVEL};
use super::spinner_tests::{frame, FRAME};
use crate::list::{bench::allocations, Palette};
use gpui::{
    div, fill, point, prelude::*, px, size, App, Bounds, Context, DevicePixels, Element, ElementId,
    GlobalElementId, Hsla, InspectorElementId, LayoutId, Pixels, Render, Style, SvgRenderer,
    SvgSize, TestAppContext, Window, WindowHandle,
};
use std::{cell::Cell, rc::Rc, sync::Arc, time::Duration};

/// What a probe window paints in its slot.
#[derive(Clone, Copy, Default)]
enum Painter {
    #[default]
    Nothing,
    /// The shipped crawl arm.
    Crawl,
    /// The bench's pinned arm at this offset.
    Pinned(f32),
    /// The dash at this phase drawn straight from `cells`, asking for a frame or not, which is GPUI's own cost for it.
    Reference(f32, bool),
}

/// A slot alone in a window, so each paint is measured without the shell around it.
struct Probe(Rc<Crawl>, Rc<Measures>);

#[derive(Default)]
struct Measures {
    painter: Cell<Painter>,
    /// Heap allocations in the last paint.
    spent: Cell<u64>,
    painted: Cell<usize>,
}

impl Render for Probe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(Slot(Rc::clone(&self.0)))
            .child(Measured(Rc::clone(&self.0), Rc::clone(&self.1)))
    }
}

struct Measured(Rc<Crawl>, Rc<Measures>);

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
        (window.request_layout(Style::default(), None, cx), ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut Window,
        _: &mut App,
    ) {
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let (crawl, measures) = (&self.0, &self.1);
        let before = allocations();
        match measures.painter.get() {
            Painter::Nothing => return,
            Painter::Crawl => crawl.paint(true, None, window, cx),
            Painter::Pinned(offset) => crawl.paint(false, Some(offset), window, cx),
            Painter::Reference(at, asks) => reference(crawl.slot(), crawl.ink(), at, asks, window),
        }
        measures.spent.set(allocations() - before);
        measures.painted.set(measures.painted.get() + 1);
    }
}

// The quads the mark's paint places for a square slot, with nothing of its own around them.
fn reference(slot: Bounds<Pixels>, ink: Hsla, at: f32, asks: bool, window: &mut Window) {
    let scale = window.scale_factor();
    let unit = f32::from(slot.size.width) * scale / GRID;
    let left = (f32::from(slot.origin.x) * scale).round();
    let top = (f32::from(slot.origin.y) * scale).round();
    window.with_content_mask(Some(window.content_mask()), |window| {
        cells(at, unit, |[x0, y0, x1, y1], coverage| {
            let bounds = Bounds::from_corners(
                point(px((left + x0) / scale), px((top + y0) / scale)),
                point(px((left + x1) / scale), px((top + y1) / scale)),
            );
            window.paint_quad(fill(bounds, ink.opacity(coverage)));
        });
    });
    if asks {
        window.request_animation_frame();
    }
}

type Probed = (Rc<Crawl>, Rc<Measures>, WindowHandle<Probe>);

fn probe(cx: &mut TestAppContext) -> Probed {
    let (crawl, measures) = (Rc::new(Crawl::default()), Rc::new(Measures::default()));
    let probe = Probe(Rc::clone(&crawl), Rc::clone(&measures));
    (crawl, measures, cx.add_window(|_, _| probe))
}

fn refresh(cx: &mut TestAppContext, handle: WindowHandle<Probe>) {
    cx.update_window(handle.into(), |_, window, _| window.refresh())
        .unwrap();
}

#[gpui::test]
fn the_mark_builds_no_geometry_and_allocates_nothing_after_its_first_frame(
    cx: &mut TestAppContext,
) {
    let period = (PERIOD_MS / FRAME.as_millis() as u64) as usize;
    // The shipped crawl on the test clock, then the bench's pinned arm walked through the same offsets.
    for pinned in [false, true] {
        // The mark and its reference draw in two windows in lockstep, so GPUI's own scene growth and frame request match.
        let (mark, reference) = (probe(cx), probe(cx));
        cx.update_window(mark.2.into(), |_, window, _| {
            let ink = Palette::inherited(&window.text_style()).metadata_frame;
            mark.0.show(true, ink);
            reference.0.show(false, ink);
        })
        .unwrap();
        let (mut last, mut wraps) = (0., 0);
        for drawn in 1..=period * 5 / 2 {
            let at = phase(FRAME * (drawn - 1) as u32);
            mark.1.painter.set(match pinned {
                false => Painter::Crawl,
                true => Painter::Pinned(-TRAVEL * at),
            });
            reference.1.painter.set(Painter::Reference(at, !pinned));
            if drawn == 1 || pinned {
                refresh(cx, mark.2);
                refresh(cx, reference.2);
            } else {
                frame(cx, mark.2);
                cx.update_window(reference.2.into(), |_, window, cx| {
                    window.simulate_next_frame(cx)
                })
                .unwrap();
            }
            let painted = (mark.0.counters.frames.get(), mark.1.painted.get());
            assert_eq!(
                (painted, reference.1.painted.get()),
                ((drawn, drawn), drawn)
            );
            assert!(
                (mark.0.offset() + TRAVEL * at).abs() < 1e-4,
                "frame {drawn}"
            );
            wraps += usize::from(mark.0.offset() > last);
            last = mark.0.offset();
            if drawn == 1 {
                continue;
            }
            let spent = (mark.1.spent.get(), reference.1.spent.get());
            assert_eq!(
                spent.0, spent.1,
                "frame {drawn}: the mark against GPUI's own cost"
            );
            // Past a whole period both of the window's alternating scenes have held every phase, so GPUI grows nothing.
            if pinned && drawn > period + 1 {
                assert_eq!(spent.0, 0, "frame {drawn}");
            }
        }
        assert_eq!(
            wraps, 2,
            "pinned {pinned}: the walk covers two whole periods"
        );
        eprintln!(
            "SPINNER_ALLOCS pinned={pinned} frames={} last_frame_allocs={}",
            mark.1.painted.get(),
            mark.1.spent.get()
        );
    }
}

// One 8-bit step of the reference; a between frame also allows its 4x supersampled edge, which lands on the nearest quarter pixel.
const EXACT: f32 = 1. / 255.;
const BETWEEN: f32 = 1. / 8. + EXACT;

/// The board's spiral at this dash offset, rasterized by GPUI's SVG renderer as the reference.
fn board(dashoffset: f32, side: u32) -> Vec<f32> {
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{side}" height="{side}" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="square" stroke-linejoin="miter"><path stroke-dasharray="60 54" stroke-dashoffset="{dashoffset}" d="M21 21H3V3h18v14H7V7h10v6h-6"/></svg>"#
    );
    let renderer = SvgRenderer::new(Arc::new(()));
    let parsed = renderer.parse_svg(svg.as_bytes()).unwrap();
    let pixels = DevicePixels(side as i32);
    let image = renderer
        .render_parsed(&parsed, SvgSize::ExactSize(size(pixels, pixels)))
        .unwrap();
    image
        .as_bytes(0)
        .unwrap()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| f32::from(pixel[3]) / 255.)
        .collect()
}

pub(super) fn raster(offset: f32, side: u32) -> Vec<f32> {
    let mut coverage = vec![0f32; (side * side) as usize];
    let at = (-offset / TRAVEL).rem_euclid(1.);
    cells(at, side as f32 / GRID, |[x0, y0, x1, y1], amount| {
        for y in y0 as u32..y1 as u32 {
            for x in x0 as u32..x1 as u32 {
                coverage[(y * side + x) as usize] += amount;
            }
        }
    });
    coverage.iter().map(|value| value.min(1.)).collect()
}

/// Writes the quads, the board and their difference side by side as a PGM, when the suite asks for specimens.
fn specimen(name: &str, side: u32, ours: &[f32], reference: &[f32]) {
    let Some(root) = std::env::var_os("FLEA_SPINNER_SPECIMENS") else {
        return;
    };
    let mut bytes = format!("P5\n{} {side}\n255\n", side * 3).into_bytes();
    for y in 0..side as usize {
        let row = y * side as usize..(y + 1) * side as usize;
        let difference: Vec<f32> = ours[row.clone()]
            .iter()
            .zip(&reference[row.clone()])
            .map(|(a, b)| (a - b).abs())
            .collect();
        for part in [&ours[row.clone()], &reference[row.clone()], &difference[..]] {
            bytes.extend(part.iter().map(|value| 255 - (value * 255.).round() as u8));
        }
    }
    std::fs::write(
        std::path::Path::new(&root).join(format!("{name}.pgm")),
        bytes,
    )
    .unwrap();
}

#[test]
fn the_quads_match_the_board_spiral_at_rest_and_mid_period() {
    // The board draws the mid frame (dash offset 60 grid units); "between" puts both dash ends inside device pixels.
    for (name, offset, dashoffset) in [
        ("rest", 0., 0.),
        ("mid", -27., 60.),
        ("between", -10.3, 93.4),
    ] {
        for side in [48, 60, 96] {
            let (ours, reference) = (raster(offset, side), board(dashoffset, side));
            specimen(&format!("spinner-{name}-{side}"), side, &ours, &reference);
            let worst = ours
                .iter()
                .zip(&reference)
                .map(|(a, b)| (a - b).abs())
                .fold(0., f32::max);
            let ink: f32 = reference.iter().sum();
            let drift: f32 = ours
                .iter()
                .zip(&reference)
                .map(|(a, b)| (a - b).abs())
                .sum();
            eprintln!("SPINNER_BOARD {name} {side} worst={worst:.4} drift={drift:.3} ink={ink:.1}");
            assert!(
                worst <= if name == "between" { BETWEEN } else { EXACT } && drift <= ink * 0.005,
                "{name} {side}: worst {worst} drift {drift} of {ink}"
            );
        }
    }
}

#[test]
fn a_period_ends_where_it_started() {
    assert_eq!(phase(Duration::from_millis(PERIOD_MS)), 0.);
    assert_eq!(raster(0., 48), raster(-TRAVEL, 48));
}
