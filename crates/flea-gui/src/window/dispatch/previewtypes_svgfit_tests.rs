//! The live Columns preview fills its frame with a small vector and keeps a small raster at its own size (v0.3.12 PreviewColumn.qml).
use super::previewtypes_kinds_tests::Root;
use super::previewtypes_tests::{answer, file, local, meta, open};
use crate::window::u07_tests::read;
use gpui::{TestAppContext, VisualTestContext};
use std::time::Duration;

/// Frames and real time for the picture's decode, which runs on the background executor.
fn settle(cx: &mut VisualTestContext) {
    for _ in 0..40 {
        cx.executor().advance_clock(Duration::from_millis(300));
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// The drawn picture's size and the frame room it sits in, for one 60 by 40 image file under the given row name.
fn drawn(cx: &mut TestAppContext, tag: &str, name: &str, icon: &str) -> ((f32, f32), (f32, f32)) {
    cx.executor().allow_parking();
    let root = Root::new(tag);
    let dir = root.0.to_string_lossy().into_owned();
    let picture = root.0.join("picture.png");
    image::RgbaImage::from_pixel(60, 40, image::Rgba([255; 4]))
        .save(&picture)
        .unwrap();
    let (shell, cx) = open(cx, &dir);
    super::test_shell::land(
        &shell,
        cx,
        &dir,
        vec![file(name, icon, "Image", 0o100644)],
        1,
    );
    let lines = local(&shell, cx, &dir);
    meta(&shell, cx, &lines, r#","w":60,"h":40"#);
    answer(
        &shell,
        cx,
        &format!(
            r#"{{"t":"thumbed","row":0,"file":"{}"}}"#,
            picture.display()
        ),
    );
    settle(cx);
    assert_eq!(
        read(&shell, cx, "previewColumnState", &[]),
        "image",
        "{name}"
    );
    let image = cx
        .debug_bounds("column-preview-image")
        .expect("the picture is drawn");
    let room = cx
        .debug_bounds("column-preview-picture")
        .expect("the frame is drawn");
    (
        (f32::from(image.size.width), f32::from(image.size.height)),
        (f32::from(room.size.width), f32::from(room.size.height)),
    )
}

#[gpui::test]
fn a_small_svg_fills_the_live_column_frame(cx: &mut TestAppContext) {
    let ((w, h), (room_w, room_h)) = drawn(cx, "svgfit-svg", "logo.svg", "image-svg+xml");
    assert!(w > 60. && h > 40., "the vector is enlarged: {w}x{h}");
    assert!(
        (w - room_w).abs() <= 1. || (h - room_h).abs() <= 1.,
        "the vector fills the frame along its limiting axis: {w}x{h} in {room_w}x{room_h}"
    );
    assert!(((w / h) - 1.5).abs() < 0.05, "the aspect is kept: {w}x{h}");
}

#[gpui::test]
fn a_small_png_keeps_its_size_in_the_live_column_frame(cx: &mut TestAppContext) {
    let ((w, h), _) = drawn(cx, "svgfit-png", "logo.png", "image-png");
    assert_eq!((w, h), (60., 40.), "a raster is never enlarged");
}
