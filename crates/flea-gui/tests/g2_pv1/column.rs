//! The preview column boards: preview-column, multi-facts, cols-preview-image and cols-preview-raw.
use super::boards::rect;
use super::*;
use flea::ui_logic_preview_core::facts::Fact;
use flea_gui::g2::quick_look_overlay::{
    column::{column, Column, Facts, Frame},
    glyph::Glyph,
    Look, RasterState,
};
use gpui::{DevicePixels, ImgResourceLoader, RenderImage, Resource, SvgSize};

struct Host {
    view: Box<dyn Fn() -> Column>,
}

impl Render for Host {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let look = Look::of_text(&window.text_style(), flea_gui::palette::Theme::default());
        // The third column of the boards: 192 px of rail, two 302 px columns, the chrome and status bars.
        let slot = div()
            .absolute()
            .left(px(796.))
            .top(px(27.))
            .w(px(304.))
            .h(px(646.));
        div()
            .size_full()
            .relative()
            .child(slot.child(column((self.view)(), &look)))
    }
}

fn draw(cx: &mut TestAppContext, view: impl Fn() -> Column + 'static) -> &mut VisualTestContext {
    let (_, cx) = cx.add_window_view(move |_, _| Host {
        view: Box::new(view),
    });
    cx.simulate_resize(size(px(1100.), px(700.)));
    cx.run_until_parked();
    cx
}

fn facts(pairs: &[(&str, &str)]) -> Facts {
    Facts::Table(
        pairs
            .iter()
            .map(|(l, v)| Fact {
                label: (*l).into(),
                value: (*v).into(),
            })
            .collect(),
    )
}

#[gpui::test]
fn cols_preview_image_fits_the_thumbnail_to_the_frame(cx: &mut TestAppContext) {
    let view = move || Column {
        width: 304.,
        raster: RasterState::default(),
        name: "photo.png".into(),
        frame: Frame::Picture {
            path: repo("docs/images/columns.png"),
            w: 800.,
            h: 600.,
        },
        facts: facts(&[
            ("Kind", "PNG image"),
            ("Size", "2.5 MB"),
            ("Modified", "2025-06-15 10:00"),
            ("Pixels", "800 \u{d7} 600"),
        ]),
    };
    let cx = draw(cx, view);
    assert_eq!(rect(cx, "pv1-column-frame"), (810., 41., 276., 173.));
    assert_eq!(
        rect(cx, "pv1-column-picture"),
        (811. + 23., 42., 228., 171.)
    );
    // the name 9 px under the frame, then the facts: four 18 px rows 3 px apart
    assert_eq!(
        rect(cx, "pv1-column-facts"),
        (810., 41. + 173. + 9. + 20. + 9., 276., 4. * 18. + 3. * 3.)
    );
}

#[gpui::test]
fn cols_small_svg_fills_the_frame_and_small_png_keeps_its_size(cx: &mut TestAppContext) {
    // EY-64: a 60x40 vector fills the 274x171 frame along the limiting axis, aspect kept.
    let view = move || Column {
        width: 304.,
        raster: RasterState::default(),
        name: "image-svg.svg".into(),
        frame: Frame::Picture {
            path: repo("docs/images/glyphs.svg"),
            w: 60.,
            h: 40.,
        },
        facts: facts(&[("Kind", "SVG image")]),
    };
    let cx = draw(cx, view);
    assert_eq!(rect(cx, "pv1-column-frame"), (810., 41., 276., 173.));
    assert_eq!(rect(cx, "pv1-column-picture"), (820., 42., 256., 171.));
    // A 60x40 raster keeps v0.3.12's rule: drawn at its own pixels.
    let view = move || Column {
        width: 304.,
        raster: RasterState::default(),
        name: "photo.png".into(),
        frame: Frame::Picture {
            path: repo("docs/images/columns.png"),
            w: 60.,
            h: 40.,
        },
        facts: facts(&[("Kind", "PNG image")]),
    };
    let cx = draw(cx, view);
    assert_eq!(rect(cx, "pv1-column-picture"), (918., 42., 60., 40.));
}

#[gpui::test]
fn cols_preview_raw_centres_its_floored_width(cx: &mut TestAppContext) {
    let view = move || Column {
        width: 304.,
        raster: RasterState::default(),
        name: "L1000921.DNG".into(),
        frame: Frame::Picture {
            path: repo("docs/images/columns.png"),
            w: 6000.,
            h: 4000.,
        },
        facts: facts(&[("Kind", "Adobe DNG negative"), ("Size", "48.1 MB")]),
    };
    let cx = draw(cx, view);
    assert_eq!(rect(cx, "pv1-column-picture"), (811. + 9., 42., 256., 171.));
}

#[gpui::test]
fn multi_facts_stacks_one_mark_per_kind(cx: &mut TestAppContext) {
    let view = move || Column {
        width: 304.,
        raster: RasterState::default(),
        name: crate::test_catalogs::english().trn_args("preview.selected", 2, &["2"]),
        frame: Frame::Stack(vec![
            Glyph::kind("text-x-generic", 0o100644),
            Glyph::kind("image-x-generic", 0o100644),
        ]),
        facts: facts(&[("Kinds", "1 text, 1 image"), ("Combined", "2.5 MB")]),
    };
    let cx = draw(cx, view);
    // a 50 by 44 stack centred in the 173 px frame
    let (fx, fy, fw, fh) = rect(cx, "pv1-column-frame");
    assert_eq!(
        rect(cx, "pv1-column-stack"),
        (fx + (fw - 50.) / 2., fy + (fh - 44.) / 2., 50., 44.)
    );
}

/// The picture the U3 host will hold: glyphs.svg rasterized at the column frame's device size.
fn rasterize(cx: &mut TestAppContext, w: u32, h: u32) -> Arc<RenderImage> {
    let renderer = cx.update(|cx| cx.svg_renderer());
    let bytes = std::fs::read(repo("docs/images/glyphs.svg")).unwrap();
    let parsed = renderer.parse_svg(&bytes).unwrap();
    let at = size(DevicePixels(w as i32), DevicePixels(h as i32));
    renderer
        .render_parsed(&parsed, SvgSize::ExactSize(at))
        .unwrap()
}

#[gpui::test]
fn the_host_raster_draws_instead_of_an_svg_file(cx: &mut TestAppContext) {
    // The column host holds a pv1 RasterState keyed by device size; the column draws that, not img(path).
    let image = rasterize(cx, 228, 171);
    let path = repo("docs/images/glyphs.svg");
    let at = path.clone();
    let view = move || Column {
        width: 304.,
        raster: RasterState {
            size: Some((228, 171)),
            image: Some(image.clone()),
            refusal: None,
        },
        name: "mark.svg".into(),
        frame: Frame::Picture {
            path: at.clone(),
            w: 800.,
            h: 600.,
        },
        facts: facts(&[("Kind", "SVG image")]),
    };
    let cx = draw(cx, view);
    assert_eq!(
        rect(cx, "pv1-column-picture"),
        (811. + 23., 42., 228., 171.)
    );
    let res = Resource::Path(Arc::from(path.as_path()));
    assert!(
        !cx.update(|_, cx| cx.has_asset::<ImgResourceLoader>(&res)),
        "the column handed a vector to img(path)"
    );
}

#[gpui::test]
fn a_vector_without_a_host_raster_draws_no_file(cx: &mut TestAppContext) {
    let path = repo("docs/images/glyphs.svg");
    let at = path.clone();
    let view = move || Column {
        width: 304.,
        raster: RasterState::default(),
        name: "mark.svg".into(),
        frame: Frame::Picture {
            path: at.clone(),
            w: 800.,
            h: 600.,
        },
        facts: facts(&[]),
    };
    let cx = draw(cx, view);
    assert!(
        cx.debug_bounds("pv1-column-picture").is_some(),
        "the frame reserves the fit"
    );
    let res = Resource::Path(Arc::from(path.as_path()));
    assert!(
        !cx.update(|_, cx| cx.has_asset::<ImgResourceLoader>(&res)),
        "a vector with no host raster fell back to img(path)"
    );
}

#[gpui::test]
fn preview_column_runs_a_document_to_the_column_bottom(cx: &mut TestAppContext) {
    let view = move || Column {
        width: 304.,
        raster: RasterState::default(),
        name: "guide.md".into(),
        frame: Frame::Document(None),
        facts: Facts::Line {
            kind: "Markdown document".into(),
            tail: " \u{b7} 298 B \u{b7} 25 lines".into(),
        },
    };
    let cx = draw(cx, view);
    // 14 px inset, the name and the 18 px line under the frame, 9 px apart
    assert_eq!(
        rect(cx, "pv1-column-frame"),
        (810., 41., 276., 646. - 28. - 9. - 20. - 9. - 18.)
    );
    assert_eq!(rect(cx, "pv1-column-facts").3, 18.);
}
