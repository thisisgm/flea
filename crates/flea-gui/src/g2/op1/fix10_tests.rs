//! Real font and raster controls anchored to the Copy To board and native finding.
use super::{host::Overlay, palette::CardPalette};
use crate::{list::bench::bench_typeface, palette::Theme};
use gpui::{
    point, px, size, App, Bounds, Global, Pixels, RenderGlyphParams, ShapedLine, TestAppContext,
    TestDispatcher, VisualTestContext, Window,
};
use std::{borrow::Cow, sync::Arc};
#[path = "glyph_baseline26_tests.rs"]
mod glyph_baseline;

#[derive(Clone, Debug)]
pub(super) struct Sample {
    pub text: String,
    pub displayed: String,
    pub bounds: Bounds<Pixels>,
    pub line: Pixels,
    pub ascent: Pixels,
    pub descent: Pixels,
    pub baseline: Pixels,
    pub ink: Vec<(usize, i32, i32)>,
}
#[derive(Default)]
struct Samples(Vec<Sample>);
impl Global for Samples {}
struct RasterFont(Arc<dyn gpui::PlatformTextSystem>);
impl Global for RasterFont {}

pub(super) fn real_app() -> TestAppContext {
    let font = gpui_platform::current_platform(true).text_system();
    let cx = TestAppContext::build_with_text_system(TestDispatcher::new(0), None, font.clone());
    cx.update(|cx| cx.set_global(RasterFont(font)));
    cx
}

pub(super) fn record(
    text: &str,
    bounds: Bounds<Pixels>,
    height: Pixels,
    line: &ShapedLine,
    window: &Window,
    cx: &mut App,
) {
    if cx.try_global::<Samples>().is_none() {
        return;
    }
    let baseline = window.pixel_snap(bounds.origin.y + (height + line.ascent - line.descent) / 2.);
    let mut ink = Vec::new();
    let font = cx.global::<RasterFont>().0.clone();
    for run in &line.runs {
        for glyph in &run.glyphs {
            let params = RenderGlyphParams {
                font_id: run.font_id,
                glyph_id: glyph.id,
                font_size: line.font_size,
                subpixel_variant: point(0, 0),
                scale_factor: window.scale_factor(),
                is_emoji: false,
                subpixel_rendering: false,
                dilation: 0,
            };
            let raster = font.glyph_raster_bounds(&params).unwrap();
            if raster.size.width.0 == 0 || raster.size.height.0 == 0 {
                continue;
            }
            let (size, bytes) = font.rasterize_glyph(&params, raster).unwrap();
            if size.width.0 == 0 {
                continue;
            }
            let rows: Vec<_> = bytes
                .chunks(size.width.0 as usize)
                .enumerate()
                .filter_map(|(y, row)| row.iter().any(|a| *a != 0).then_some(y as i32))
                .collect();
            if let (Some(first), Some(last)) = (rows.first(), rows.last()) {
                let origin = f32::from(baseline + glyph.position.y) * window.scale_factor();
                let y = origin.round() as i32 + raster.origin.y.0;
                ink.push((glyph.index, y + first, y + last));
            }
        }
    }
    cx.global_mut::<Samples>().0.push(Sample {
        text: text.into(),
        displayed: line.text.to_string(),
        bounds,
        line: height,
        ascent: line.ascent,
        descent: line.descent,
        baseline,
        ink,
    });
}

pub(super) fn init(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Samples::default());
        cx.set_global(bench_typeface());
        cx.text_system()
            .add_fonts(vec![
                Cow::Borrowed(
                    include_bytes!("../../../tests/fonts/JetBrainsMono-Regular.ttf").as_slice(),
                ),
                Cow::Borrowed(
                    include_bytes!("../../../tests/fonts/JetBrainsMono-Bold.ttf").as_slice(),
                ),
            ])
            .unwrap();
        assert!(cx
            .text_system()
            .all_font_names()
            .iter()
            .any(|name| name == "JetBrains Mono"));
    });
}

pub(super) fn redraw(cx: &mut VisualTestContext) -> Vec<Sample> {
    cx.update(|window, cx| {
        cx.set_global(Samples::default());
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.read(|cx| cx.global::<Samples>().0.clone())
}

fn open<'a>(cx: &'a mut TestAppContext, base: f32, path: &str) -> &'a mut VisualTestContext {
    init(cx);
    // A window without a Shell root takes the installed face, so the text size rides on it.
    cx.update(|cx| cx.set_global(bench_typeface().at_base(base as u32)));
    let (view, cx) = cx.add_window_view(|window, _| {
        window.set_rem_size(px(base));
        let palette = CardPalette::from_theme(&window.text_style(), Theme::default(), 8.);
        Overlay::new(Arc::new(super::tests::catalog()), palette, "default")
    });
    cx.simulate_resize(size(px(1100.), px(700.)));
    view.update(cx, |view, cx| view.copy_to(0, false, path, cx));
    cx.run_until_parked();
    cx
}

#[test]
fn fix10_native_card_geometry_and_button_baseline() {
    for path in ["/fx/v/work", ""] {
        let mut app = real_app();
        let cx = open(&mut app, 14., path);
        cx.simulate_scale_factor_change(1.);
        let samples = redraw(cx);
        let cancel = samples.iter().find(|s| s.text == "Cancel").unwrap();
        let copy = samples.iter().find(|s| s.text == "Copy").unwrap();
        assert_eq!(cancel.baseline, copy.baseline, "same font baseline");
        assert_eq!(cancel.baseline, px(412.), "board baseline");
        for label in [cancel, copy] {
            assert_eq!(
                label.ink[0],
                (0, 402, 411),
                "capital C in board/native crop"
            );
        }
        assert_eq!(copy.ink[2].2, 414, "p descends three pixels below C");
        for (name, y) in [
            ("op1-rule", 309.),
            ("op1-field", 341.),
            ("op1-button-0", 393.),
        ] {
            assert_eq!(
                cx.debug_bounds(name).unwrap().origin.y,
                px(y),
                "board {name}"
            );
        }
        let card = cx.debug_bounds("op1-card").unwrap();
        assert_eq!((card.size.width, card.bottom()), (px(552.), px(440.)));
        let button = cx.debug_bounds("op1-button-0").unwrap();
        assert_eq!(button.size, size(px(71.), px(30.)));
        let quads = cx.update(|window, _| window.painted_quads());
        assert!(
            quads.iter().any(|q| q.bounds == card.scale(1.)),
            "card frame is painted at its measured bounds"
        );
        if !path.is_empty() {
            assert_eq!(
                glyph_baseline::actual_path_baseline(path),
                px(360.),
                "board path ink begins at 348"
            );
        }
        println!("path={path:?}: rule=309 field=341 buttons=393 bottom=439 C=402..411 p=414 baseline=412");
    }
}

#[test]
fn fix10_real_font_card_geometry_at_each_theme_stop() {
    for (base, width, control, height, title, body) in [
        (12., 473., 26., 155., 14., 12.),
        (14., 552., 30., 180., 16., 14.),
        (16., 630., 35., 206., 18., 16.),
    ] {
        let mut app = real_app();
        let cx = open(&mut app, base, "/fx/v/work");
        for scale in [1., 2.] {
            cx.simulate_scale_factor_change(scale);
            let samples = redraw(cx);
            let card = cx.debug_bounds("op1-card").unwrap();
            assert_eq!(card.size.width, px(width));
            if scale == 1. {
                assert_eq!(card.size.height, px(height));
            }
            assert_eq!(
                cx.debug_bounds("op1-button-0").unwrap().size.height,
                px(control)
            );
            assert_eq!(
                cx.debug_bounds("op1-field").unwrap().size.height,
                px(control)
            );
            let heading = samples.iter().find(|s| s.text == "Copy To").unwrap();
            let cancel = samples.iter().find(|s| s.text == "Cancel").unwrap();
            let copy = samples.iter().find(|s| s.text == "Copy").unwrap();
            assert_eq!(heading.line, px(title * 1.2_f32));
            assert_eq!(cancel.line, px(body * 1.2_f32));
            assert_eq!(cancel.ascent, px(body * 1.02_f32));
            assert_eq!(cancel.descent, px(body * 0.3_f32));
            assert_eq!(cancel.baseline, copy.baseline);
            assert!(cancel.bounds.top() >= cx.debug_bounds("op1-button-0").unwrap().top());
            assert!(cancel.bounds.bottom() <= cx.debug_bounds("op1-button-0").unwrap().bottom());
            println!(
                "body={base} scale={scale} card={:?} baseline={:?}",
                card.size, cancel.baseline
            );
        }
    }
}
