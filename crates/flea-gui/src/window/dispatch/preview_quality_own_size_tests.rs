//! A small raster in the column keeps its own pixels, one to each device pixel and centred.
use super::{
    preview_quality_tests::{column, wait, Root},
    previewtypes_tests::{file, local, meta, open, thumbed},
};
use crate::window::u07_tests::read;
use gpui::TestAppContext;

// A small raster draws at its own pixels, one to each device pixel and centred, decoded from the original.
fn column_keeps_own_pixels(cx: &mut TestAppContext, source: image::RgbaImage, thumb: (u32, u32)) {
    cx.executor().allow_parking();
    let root = Root::new();
    let dir = root.0.to_string_lossy().into_owned();
    let (w, h) = source.dimensions();
    source.save(root.0.join("small.png")).unwrap();
    image::RgbaImage::from_pixel(thumb.0, thumb.1, image::Rgba([128, 128, 128, 255]))
        .save(root.0.join("small-thumb.png"))
        .unwrap();
    let (shell, cx) = open(cx, &dir);
    cx.simulate_resize(gpui::size(gpui::px(1400.), gpui::px(900.)));
    super::test_shell::land(
        &shell,
        cx,
        &dir,
        vec![file("small.png", "image-x-generic", "PNG image", 0o100644)],
        1,
    );
    let lines = local(&shell, cx, &dir);
    meta(&shell, cx, &lines, &format!(r#","w":{w},"h":{h}"#));
    thumbed(
        &shell,
        cx,
        &lines,
        0,
        &root.0.join("small-thumb.png").display().to_string(),
    );
    for scale in [1.25f32, 1.6, 2.] {
        cx.simulate_scale_factor_change(scale);
        wait(cx, |cx| {
            column(&shell, cx) == Some((w, h))
                && read(&shell, cx, "previewQuality", &[]).contains(r#""refined":true"#)
        });
        let pixels = shell.read_with(cx, |shell, cx| {
            let view = shell.selection_preview(0).unwrap().read(cx);
            let image = view.drawn.as_ref().unwrap().image.clone().unwrap();
            image.as_bytes(0).unwrap().to_vec()
        });
        assert!(
            pixels
                .as_chunks::<4>()
                .0
                .iter()
                .copied()
                .zip(source.pixels())
                .all(|(bgra, own)| bgra == [own[2], own[1], own[0], own[3]]),
            "the original's own pixels at {scale}, never the thumbnail's"
        );
        let rect = cx.debug_bounds("column-preview-image").unwrap();
        let frame = cx.debug_bounds("column-preview-picture").unwrap();
        for (side, want) in [(rect.size.width, w), (rect.size.height, h)] {
            let drawn = f32::from(side) * scale;
            assert!(
                drawn <= want as f32 + 1e-3 && drawn > want as f32 - 0.01,
                "{drawn} device pixels for {want} at {scale}"
            );
        }
        // The image's bounds are snapped to device pixels, so an odd leftover leaves one device pixel more on one side.
        let gap =
            |inner: gpui::Pixels, outer: gpui::Pixels, at: gpui::Pixels, from: gpui::Pixels| {
                f32::from(at - from) * 2. - f32::from(outer - inner)
            };
        let slack = 1. / scale + 1e-3;
        assert!(
            gap(
                rect.size.width,
                frame.size.width,
                rect.origin.x,
                frame.origin.x
            )
            .abs()
                <= slack
                && gap(
                    rect.size.height,
                    frame.size.height,
                    rect.origin.y,
                    frame.origin.y
                )
                .abs()
                    <= slack,
            "centred at {scale}: {rect:?} in {frame:?}"
        );
    }
}

#[gpui::test]
fn preview_quality_column_pixel_art_stays_at_its_own_pixels(cx: &mut TestAppContext) {
    let art = image::RgbaImage::from_fn(64, 64, |x, y| {
        image::Rgba(if (x + y) % 2 == 0 {
            [0, 0, 0, 255]
        } else {
            [255, 255, 255, 255]
        })
    });
    column_keeps_own_pixels(cx, art, (64, 64));
}

#[gpui::test]
fn preview_quality_column_small_photo_stays_at_its_own_pixels(cx: &mut TestAppContext) {
    let photo = image::RgbaImage::from_fn(320, 240, |x, y| {
        image::Rgba([x as u8, y as u8, (x ^ y) as u8, 255])
    });
    column_keeps_own_pixels(cx, photo, (256, 192));
}
