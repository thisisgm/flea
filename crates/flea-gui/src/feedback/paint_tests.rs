use super::spinner::{cells, phase, Crawl, Slot, PERIOD_MS};
use crate::list::{bench::allocations, Palette};
use gpui::{
    div, prelude::*, size, App, Bounds, Context, DevicePixels, Element, ElementId, GlobalElementId,
    InspectorElementId, LayoutId, Pixels, Render, Style, SvgRenderer, SvgSize, TestAppContext,
    Window,
};
use std::{cell::Cell, rc::Rc, sync::Arc, time::Duration};

/// A slot alone in a window, so one paint of the mark is measured without the shell around it.
struct Probe {
    crawl: Rc<Crawl>,
    offset: Rc<Cell<f32>>,
    cost: Rc<Cell<u64>>,
}

impl Render for Probe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(Slot(Rc::clone(&self.crawl)))
            .child(Measured(
                Rc::clone(&self.crawl),
                self.offset.get(),
                Rc::clone(&self.cost),
            ))
    }
}

struct Measured(Rc<Crawl>, f32, Rc<Cell<u64>>);

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
        let before = allocations();
        self.0.paint(false, Some(self.1), window, cx);
        self.2.set(self.2.get() + allocations() - before);
    }
}

#[gpui::test]
fn the_mark_builds_no_geometry_and_allocates_nothing_after_its_first_frame(
    cx: &mut TestAppContext,
) {
    let crawl = Rc::new(Crawl::default());
    let (offset, cost) = (Rc::new(Cell::new(0.)), Rc::new(Cell::new(0)));
    let probe = Probe {
        crawl: Rc::clone(&crawl),
        offset: Rc::clone(&offset),
        cost: Rc::clone(&cost),
    };
    let handle = cx.add_window(|_, _| probe);
    cx.update_window(handle.into(), |_, window, _| {
        crawl.show(
            true,
            Palette::inherited(&window.text_style()).metadata_frame,
        );
    })
    .unwrap();
    let draw = |cx: &mut TestAppContext, at: f32| {
        offset.set(at);
        cx.update_window(handle.into(), |_, window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        })
        .unwrap();
    };
    let steps = PERIOD_MS / 8;
    let period = |cx: &mut TestAppContext| {
        for step in 0..steps {
            draw(cx, -57. * step as f32 / steps as f32);
        }
    };
    period(cx);
    let painted = crawl.counters.frames.get();
    cost.set(0);
    period(cx);
    assert_eq!(cost.get(), 0);
    assert_eq!((crawl.counters.frames.get() - painted) as u64, steps);
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
    let at = (-offset / 57.).rem_euclid(1.);
    cells(at, side as f32 / 24., |[x0, y0, x1, y1], amount| {
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
    assert_eq!(raster(0., 48), raster(-57., 48));
}
