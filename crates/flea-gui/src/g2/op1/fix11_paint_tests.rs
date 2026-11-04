//! Pixels from GPUI's existing WGPU screenshot path, downstream of Line::paint.
use super::{host::Overlay, palette::CardPalette};
use crate::{list::bench::bench_typeface, palette::Theme};
use gpui::{px, size, AppContext, HeadlessAppContext, TextRenderingMode};
use image::{ImageFormat, RgbaImage};
use std::{borrow::Cow, fmt::Write, io::Cursor, sync::Arc};

pub(super) fn app() -> HeadlessAppContext {
    let mut app = HeadlessAppContext::with_platform(
        gpui_platform::current_platform(true).text_system(),
        Arc::new(()),
        gpui_platform::current_headless_renderer,
    );
    app.update(|cx| {
        cx.set_global(bench_typeface());
        cx.set_global(Theme::from_text("background='#1c120d'\nforeground='#c6b395'\naccent='#d28a41'\ndark_background='#150d0a'"));
        cx.set_text_rendering_mode(TextRenderingMode::Grayscale);
        cx.text_system().add_fonts(vec![
            Cow::Borrowed(include_bytes!("../../../tests/fonts/JetBrainsMono-Regular.ttf").as_slice()),
            Cow::Borrowed(include_bytes!("../../../tests/fonts/JetBrainsMono-Bold.ttf").as_slice()),
        ]).unwrap();
    });
    app
}

fn rows(image: &RgbaImage, x: std::ops::Range<u32>, y: std::ops::Range<u32>) -> Vec<u32> {
    let bg = image.get_pixel(x.start, y.start);
    y.filter(|y| {
        x.clone().any(|x| {
            image.get_pixel(x, *y).0[..3]
                .iter()
                .zip(&bg.0[..3])
                .any(|(a, b)| a.abs_diff(*b) > 1)
        })
    })
    .collect()
}

fn artifact(image: &RgbaImage, name: &str) {
    let crop = image::imageops::crop_imm(image, 660, 385, 160, 55).to_image();
    let mut bytes = Cursor::new(Vec::new());
    crop.write_to(&mut bytes, ImageFormat::Png).unwrap();
    let mut hex = String::new();
    for byte in bytes.into_inner() {
        write!(&mut hex, "{byte:02x}").unwrap();
    }
    println!("FIX11_PNG {name} {hex}");
}

#[test]
fn fix11_actual_copy_to_paint_has_the_oracle_capital_and_descender_rows() {
    let mut app = app();
    for (name, path) in [("populated", "/fx/v/work"), ("empty", "")] {
        let handle = app
            .open_window(size(px(1100.), px(700.)), |window, cx| {
                window.set_scale_factor(1.);
                window.set_rem_size(px(14.));
                let palette =
                    CardPalette::from_theme(&window.text_style(), *cx.global::<Theme>(), 8.);
                cx.new(|cx| {
                    let mut overlay =
                        Overlay::new(Arc::new(super::tests::catalog()), palette, "default");
                    overlay.copy_to(0, false, path, cx);
                    overlay
                })
            })
            .unwrap();
        app.run_until_parked();
        app.update_window(handle.into(), |_, window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        })
        .unwrap();
        let image = app
            .capture_screenshot(handle.into())
            .expect("the stock GPUI renderer must capture actual paint");
        artifact(&image, name);
        let cancel_c = rows(&image, 685..695, 396..420);
        let copy_c = rows(&image, 765..775, 396..420);
        let copy = rows(&image, 765..799, 396..420);
        println!(
            "actual {name}: image={:?} Cancel C={cancel_c:?} Copy C={copy_c:?} Copy={copy:?}",
            image.dimensions()
        );
        assert_eq!(
            cancel_c,
            (402..=411).collect::<Vec<_>>(),
            "actual Cancel C pixels"
        );
        assert_eq!(copy_c, cancel_c, "actual Copy capital shares the baseline");
        assert_eq!(copy.last(), Some(&414), "actual p descends three pixels");
    }
}
