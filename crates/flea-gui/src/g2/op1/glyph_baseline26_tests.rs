//! A reference raster measures the mounted editor's baseline from actual WGPU pixels.
use super::{CardPalette, Overlay};
use gpui::{canvas, div, point, prelude::*, px, size, AppContext, GlyphRun, Hsla, Pixels};
use image::RgbaImage;
use std::{collections::BTreeSet, rc::Rc, sync::Arc};

struct Reference {
    run: Rc<GlyphRun>,
    x: Pixels,
    ink: Hsla,
    background: Hsla,
}

impl gpui::Render for Reference {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        _: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        let (run, x, ink) = (self.run.clone(), self.x, self.ink);
        div().size_full().bg(self.background).child(
            canvas(
                |_, _, _| (),
                move |_, (), window, _| {
                    window
                        .paint_glyph_run(point(x, px(40.)), &run, ink)
                        .expect("actual reference glyph paint");
                },
            )
            .size_full(),
        )
    }
}

fn mask(
    image: &RgbaImage,
    rect: [u32; 4],
    background: [u8; 4],
    origin: i32,
) -> BTreeSet<(i32, i32)> {
    let [left, top, right, bottom] = rect;
    assert!(left < right && top < bottom && right <= image.width() && bottom <= image.height());
    (top..bottom)
        .flat_map(|y| (left..right).map(move |x| (x, y)))
        .filter(|(x, y)| {
            image.get_pixel(*x, *y).0[..3]
                .iter()
                .zip(&background[..3])
                .any(|(a, b)| a.abs_diff(*b) > 1)
        })
        .map(|(x, y)| (x as i32 - origin, y as i32))
        .collect()
}

pub(super) fn actual_path_baseline(path: &str) -> Pixels {
    assert_eq!(
        path, "/fx/v/work",
        "the exact board path is the glyph oracle"
    );
    let mut app = super::super::fix11_paint_tests::app();
    let handle = app
        .open_window(size(px(1100.), px(700.)), |window, cx| {
            window.set_scale_factor(1.);
            window.set_rem_size(px(14.));
            let palette = CardPalette::from_theme(
                &window.text_style(),
                *cx.global::<crate::palette::Theme>(),
                8.,
            );
            cx.new(|cx| {
                let mut view = Overlay::new(
                    Arc::new(crate::test_catalogs::english()),
                    palette,
                    "default",
                );
                view.copy_to(0, false, path, cx);
                view
            })
        })
        .unwrap();
    app.run_until_parked();
    app.update_window(handle.into(), |_, window, cx| {
        assert!(window.dispatch_keystroke(gpui::Keystroke::parse("tab").unwrap(), cx));
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
    let (run, paint, palette) = handle
        .update(&mut app, |view, window, _| {
            assert_eq!(window.scale_factor(), 1.);
            let drawn = view.editor.drawn.borrow();
            let paint = drawn
                .paint
                .clone()
                .expect("actual mounted Destination::paint");
            assert_eq!(drawn.shaped_text.as_ref(), path);
            assert_eq!(paint.text, path);
            assert!(
                paint.selection.is_none() && paint.caret.is_none(),
                "Tab removes only editor adornments"
            );
            let run = drawn.line.run.clone().expect("the actual shaped run");
            assert_eq!(run.line().len, path.len());
            assert_eq!(run.line().font_size, px(14.));
            assert!(run.resolves() > 0, "the actual run reached glyph paint");
            assert_eq!(paint.scroll, px(0.));
            (run, paint, view.palette)
        })
        .unwrap();
    let image = app
        .capture_screenshot(handle.into())
        .expect("actual editor WGPU screenshot");
    let rect = [
        f32::from(paint.mask.left()).ceil() as u32,
        f32::from(paint.mask.top()).ceil() as u32,
        f32::from(paint.mask.right()).floor() as u32,
        f32::from(paint.mask.bottom()).floor() as u32,
    ];
    assert!(
        f32::from(paint.origin + run.line().width) + 4. < rect[2] as f32,
        "the background sample is beyond the complete short path"
    );
    let background = image.get_pixel(rect[2] - 2, rect[1] + 1).0;
    let origin = f32::from(paint.origin).floor() as i32;
    let actual = mask(&image, rect, background, origin);
    assert!(
        !actual.is_empty(),
        "actual destination glyph paint is required"
    );
    let reference_x = 20. + f32::from(paint.origin) - origin as f32;
    assert!(reference_x + f32::from(run.line().width) < 398.);
    let reference = app
        .open_window(size(px(400.), px(80.)), |window, cx| {
            window.set_scale_factor(1.);
            cx.new(|_| Reference {
                run,
                x: px(reference_x),
                ink: palette.foreground,
                background: palette.background,
            })
        })
        .unwrap();
    app.run_until_parked();
    app.update_window(reference.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
    let reference_image = app
        .capture_screenshot(reference.into())
        .expect("actual reference WGPU screenshot");
    let expected = mask(
        &reference_image,
        [0, 0, 400, 80],
        reference_image.get_pixel(0, 0).0,
        reference_x.floor() as i32,
    );
    assert!(!expected.is_empty(), "reference glyph paint is required");
    // Both masks are observed pixels. No glyph-centering formula or expected baseline feeds this search.
    let translations: Vec<_> = (-700..=700)
        .filter(|dy| {
            expected
                .iter()
                .map(|(x, y)| (*x, *y + dy))
                .collect::<BTreeSet<_>>()
                == actual
        })
        .collect();
    assert_eq!(
        translations.len(),
        1,
        "the entire actual glyph mask must match one vertical translation"
    );
    println!("BASELINE26 actual_first={:?} reference_first={:?} translations={translations:?} observed_baseline={}", actual.iter().map(|(_, y)| *y).min(), expected.iter().map(|(_, y)| *y).min(), 40 + translations[0]);
    assert_eq!(
        actual.iter().map(|(_, y)| *y).min(),
        Some(348),
        "actual board path ink begins at348"
    );
    px((40 + translations[0]) as f32)
}
