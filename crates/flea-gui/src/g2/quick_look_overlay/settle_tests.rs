//! An open needs one original decode, with or without a borrowed first paint, with the exact final pixels and no enlargement.
use super::*;
use flea::ui_logic_preview_core::{kinds::State, rect::Shape};
use gpui::{px, size, TestAppContext};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        let made = std::process::Command::new("mktemp")
            .arg("-d")
            .arg(std::env::temp_dir().join("flea-ql-settle.XXXXXXXX"))
            .output()
            .unwrap();
        assert!(made.status.success());
        let root = PathBuf::from(String::from_utf8(made.stdout).unwrap().trim());
        assert!(root.is_absolute() && root.starts_with(std::env::temp_dir()));
        std::fs::write(root.join(".flea-test-sandbox"), "SOL-QL-SETTLE").unwrap();
        Self(root)
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        assert!(self.0.is_absolute() && self.0.starts_with(std::env::temp_dir()));
        assert!(self.0.join(".flea-test-sandbox").is_file());
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn check(cx: &mut TestAppContext, extension: &str, dimensions: (u32, u32), borrowed: bool) {
    cx.executor().allow_parking();
    let root = Root::new();
    let name = format!("picture.{extension}");
    let path = root.0.join(&name);
    image::RgbImage::from_fn(dimensions.0, dimensions.1, |x, y| {
        image::Rgb([
            (x * 13 + y * 7) as u8,
            (x * 3 + y * 17) as u8,
            (x + y * 5) as u8,
        ])
    })
    .save(&path)
    .unwrap();
    let (look, cx) =
        cx.add_window_view(|_, _| QuickLook::new(Arc::new(crate::test_catalogs::english())));
    cx.simulate_resize(size(px(1100.), px(700.)));
    look.update(cx, |look, cx| {
        look.open(
            Subject {
                name,
                icon: "image-jpeg".into(),
                mode: 0o100644,
                state: State::Image,
                shape: Shape {
                    kind: State::Image,
                    known: true,
                    w: dimensions.0.into(),
                    h: dimensions.1.into(),
                    orient: 1,
                },
                body: Body::Image {
                    path: path.clone(),
                    w: dimensions.0.into(),
                    h: dimensions.1.into(),
                },
            },
            true,
            cx,
        );
        if borrowed {
            let interim = Arc::new(RenderImage::from_bgra(1, 1, vec![0, 0, 240, 255]).unwrap());
            look.picture_interim(path.clone(), interim, cx);
        }
    });
    let started = Instant::now();
    loop {
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        cx.run_until_parked();
        if look.read_with(cx, |look, _| {
            look.quality_pixels().is_some_and(|(_, ready)| ready)
        }) {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "original decode did not finish"
        );
        cx.executor().advance_clock(Duration::from_millis(5));
        std::thread::sleep(Duration::from_millis(1));
    }
    let (image, target) = look.read_with(cx, |look, _| {
        assert_eq!(
            look.quality_decodes(),
            1,
            "first paint and final pixels come from one original decode"
        );
        (look.svg_raster().unwrap(), look.quality_pixels().unwrap().0)
    });
    let expected = photo::decode_sharp(&path, target, &|| true).unwrap();
    assert_eq!(
        image.as_bytes(0),
        expected.as_bytes(0),
        "final pixels must match original refinement"
    );
    assert!(
        image.size(0).width.0 <= dimensions.0 as i32
            && image.size(0).height.0 <= dimensions.1 as i32
    );
    if dimensions == (12, 9) {
        assert_eq!((image.size(0).width.0, image.size(0).height.0), (12, 9));
    }
    cx.update(|window, _| window.remove_window());
}

#[gpui::test]
fn ql_settle_jpeg_borrowed_first_paint_decodes_original_once(cx: &mut TestAppContext) {
    check(cx, "jpg", (1200, 800), true);
}

#[gpui::test]
fn ql_settle_png_borrowed_first_paint_decodes_original_once(cx: &mut TestAppContext) {
    check(cx, "png", (1200, 800), true);
}

#[gpui::test]
fn ql_settle_small_raster_keeps_its_own_pixels(cx: &mut TestAppContext) {
    check(cx, "png", (12, 9), true);
}

#[gpui::test]
fn b03_list_png_first_paint_decodes_original_once(cx: &mut TestAppContext) {
    check(cx, "png", (1200, 800), false);
}

#[gpui::test]
fn b03_list_small_raster_keeps_its_own_pixels(cx: &mut TestAppContext) {
    check(cx, "png", (12, 9), false);
}

#[gpui::test]
fn ql_settle_retains_scaled_decode_when_full_source_is_over_bound(cx: &mut TestAppContext) {
    over_bound(cx, true);
}

#[gpui::test]
fn b03_list_jpeg_retains_scaled_decode_when_full_source_is_over_bound(cx: &mut TestAppContext) {
    over_bound(cx, false);
}

fn over_bound(cx: &mut TestAppContext, borrowed: bool) {
    cx.executor().allow_parking();
    let root = Root::new();
    let path = root.0.join("large.jpg");
    image::GrayImage::new(12000, 9000).save(&path).unwrap();
    assert!(
        photo::decode(&path, (300, 225), &|| true).is_ok(),
        "scaled decode admits this original"
    );
    assert!(
        photo::decode_sharp(&path, (300, 225), &|| true).is_err(),
        "full source exceeds the existing pixel bound"
    );
    let (look, cx) =
        cx.add_window_view(|_, _| QuickLook::new(Arc::new(crate::test_catalogs::english())));
    cx.simulate_resize(size(px(1100.), px(700.)));
    let interim = Arc::new(RenderImage::from_bgra(1, 1, vec![0, 0, 240, 255]).unwrap());
    let interim_id = interim.id;
    look.update(cx, |look, cx| {
        look.open(
            Subject {
                name: "large.jpg".into(),
                icon: "image-jpeg".into(),
                mode: 0o100644,
                state: State::Image,
                shape: Shape {
                    kind: State::Image,
                    known: true,
                    w: 12000.,
                    h: 9000.,
                    orient: 1,
                },
                body: Body::Image {
                    path: path.clone(),
                    w: 12000.,
                    h: 9000.,
                },
            },
            true,
            cx,
        );
        if borrowed {
            look.picture_interim(path.clone(), interim, cx);
        }
    });
    let started = Instant::now();
    loop {
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        cx.run_until_parked();
        if look.read_with(cx, |look, _| {
            look.svg_refused()
                || look
                    .svg_raster()
                    .is_some_and(|image| image.id != interim_id)
        }) {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "original did not finish"
        );
        cx.executor().advance_clock(Duration::from_millis(5));
        std::thread::sleep(Duration::from_millis(1));
    }
    look.read_with(cx, |look, _| {
        assert!(
            !look.svg_refused(),
            "a scaled-readable original must still display"
        );
        assert!(look
            .svg_raster()
            .is_some_and(|image| image.id != interim_id));
        let quality = &look.svg.as_ref().unwrap().quality;
        assert!(
            quality.requested.is_some() || !quality.ready,
            "scaled fallback is not sharp refinement"
        );
    });
    cx.update(|window, _| window.remove_window());
}
