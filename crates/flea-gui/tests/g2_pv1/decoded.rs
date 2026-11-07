//! SW-13: TIFF, ICO and AVIF draw from the card's own raster job at the drawn size, never through img(path).
use super::boards::rect;
use super::svg::{gpui_loaded, raster_of, wait_for_raster, wait_for_selector, watchdog};
use super::*;
use flea_gui::g2::pv1::{decode, Rasterizer};
use gpui::{size, DevicePixels, RenderImage, TestAppContext, VisualTestContext};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

fn fixture(name: &str) -> PathBuf {
    repo("src/backend/imageformats_fixtures").join(name)
}

/// The fixture as the meta reply opens it: an upright picture of its header's size.
fn picture(name: &str, w: f64, h: f64) -> Subject {
    let mut subject = scenes::ql_svg();
    subject.name = name.into();
    subject.body = Body::Image {
        path: fixture(name),
        w,
        h,
    };
    subject.shape = flea::ui_logic_pv1::rect::Shape {
        kind: State::Image,
        known: true,
        w,
        h,
        orient: 1,
    };
    subject
}

/// The test window runs at scale 2: twice the logical fit, never past the source's own pixels.
fn wanted(cx: &mut VisualTestContext, w: f64, h: f64) -> gpui::Size<DevicePixels> {
    let (_, _, fw, fh) = rect(cx, "pv1-picture");
    let side =
        |fit: f32, source: f64| DevicePixels(((f64::from(fit) * 2.).ceil()).min(source) as i32);
    size(side(fw, w), side(fh, h))
}

fn drawn(cx: &mut TestAppContext, name: &str, w: f64, h: f64) {
    let _watchdog = watchdog(120);
    let path = fixture(name);
    let (cx, _, look, _) = host(cx, picture(name, w, h), Arm::Fix);
    let image = wait_for_raster(cx, &look, None);
    wait_for_selector(cx, "pv1-picture");
    assert_eq!(
        image.size(0),
        wanted(cx, w, h),
        "{name} decodes at the drawn size"
    );
    assert!(
        cx.debug_bounds("pv1-bar").is_none(),
        "{name} shows no unreadable card"
    );
    assert!(!gpui_loaded(cx, &path), "{name} was handed to img(path)");
}

#[gpui::test]
fn a_tiff_draws_its_own_raster_at_the_drawn_size(cx: &mut TestAppContext) {
    drawn(cx, "image-tiff.tiff", 320., 240.);
}

#[gpui::test]
fn an_icon_draws_its_own_raster_at_the_drawn_size(cx: &mut TestAppContext) {
    drawn(cx, "image-ico.ico", 48., 36.);
}

#[gpui::test]
fn an_avif_draws_through_libheif_at_the_drawn_size(cx: &mut TestAppContext) {
    if !decode::heif_present(decode::LIBHEIF) {
        eprintln!("skip: {} is not installed on this box", decode::LIBHEIF);
        return;
    }
    assert!(
        decode::heif_loads(decode::LIBHEIF),
        "{} opens with every entry point",
        decode::LIBHEIF
    );
    drawn(cx, "image-avif.avif", 320., 240.);
}

#[gpui::test]
fn a_missing_libheif_shows_the_unreadable_card(cx: &mut TestAppContext) {
    let _watchdog = watchdog(120);
    let path = fixture("image-avif.avif");
    let (cx, _, look, _) = host(cx, scenes::ql_svg(), Arm::Fix);
    let _ = wait_for_raster(cx, &look, None);
    look.update(cx, |look, cx| {
        look.set_svg_raster(decode::heif_raster("libheif-flea-absent.so.1"));
        look.close(cx);
        look.open(picture("image-avif.avif", 320., 240.), true, cx);
    });
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    wait_for_selector(cx, "pv1-recipe");
    assert!(
        cx.debug_bounds("pv1-bar").is_some(),
        "the unreadable card keeps its error bar"
    );
    assert!(
        raster_of(cx, &look).is_none(),
        "no raster lands without the library"
    );
    assert!(
        cx.debug_bounds("pv1-picture").is_none(),
        "no picture draws without the library"
    );
    assert!(!gpui_loaded(cx, &path), "the AVIF was handed to img(path)");
}

#[gpui::test]
fn a_host_decoded_picture_decodes_off_the_ui_thread(cx: &mut TestAppContext) {
    let _watchdog = watchdog(120);
    let (cx, _, look, _) = host(cx, picture("image-tiff.tiff", 320., 240.), Arm::Fix);
    let drawn = wait_for_raster(cx, &look, None);
    let calls = Arc::new(AtomicUsize::new(0));
    let heard = calls.clone();
    let slow = drawn.clone();
    let start = Instant::now();
    cx.update(|window, cx| {
        look.update(cx, |look, cx| {
            look.set_svg_raster(Arc::new(move |_bytes, _size| {
                std::thread::sleep(Duration::from_millis(400));
                heard.fetch_add(1, Ordering::SeqCst);
                Ok(slow.clone())
            }));
            look.close(cx);
            look.open(picture("image-ico.ico", 48., 36.), true, cx);
        });
        window.draw(cx).clear(cx);
    });
    let frame = start.elapsed();
    assert!(
        frame < Duration::from_millis(150),
        "the UI update and frame took {frame:?}"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "the UI frame decoded the picture itself"
    );
    let landed = wait_for_raster(cx, &look, None);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "the executor decoded the picture once"
    );
    assert_eq!(
        landed.id.0, drawn.id.0,
        "the injected raster is the one drawn"
    );
}

fn decoded(raster: &Rasterizer, bytes: &[u8], at: (u32, u32)) -> Arc<RenderImage> {
    raster(bytes, at).unwrap_or_else(|e| panic!("decodes: {e}"))
}

#[test]
fn a_large_source_is_held_at_the_requested_size_not_its_own() {
    let (w, h) = (4000u32, 3000u32);
    let big = image::RgbaImage::from_fn(w, h, |x, y| image::Rgba([x as u8, y as u8, 7, 255]));
    let mut tiff = std::io::Cursor::new(Vec::new());
    big.write_to(&mut tiff, image::ImageFormat::Tiff).unwrap();
    let image = decoded(&decode::picture_raster(), tiff.get_ref(), (400, 300));
    assert_eq!(
        image.size(0),
        size(DevicePixels(400), DevicePixels(300)),
        "12 MP is held at 400x300"
    );
    let icon = std::fs::read(fixture("image-ico.ico")).unwrap();
    assert_eq!(
        decoded(&decode::picture_raster(), &icon, (48, 36)).size(0),
        size(DevicePixels(48), DevicePixels(36))
    );
    if decode::heif_present(decode::LIBHEIF) {
        let avif = std::fs::read(fixture("image-avif.avif")).unwrap();
        let half = decoded(&decode::heif_raster(decode::LIBHEIF), &avif, (160, 120));
        assert_eq!(
            half.size(0),
            size(DevicePixels(160), DevicePixels(120)),
            "the AVIF is held at the drawn size"
        );
    } else {
        eprintln!("skip: {} is not installed on this box", decode::LIBHEIF);
    }
}

#[test]
fn a_decoder_refuses_what_is_not_its_picture() {
    assert!(decode::picture_raster()(b"not a picture", (10, 10)).is_err());
    assert!(
        decode::heif_raster("libheif-flea-absent.so.1")(b"\0\0\0\x20ftypavif", (10, 10)).is_err()
    );
    if decode::heif_present(decode::LIBHEIF) {
        // An installed libheif must open whole, so the refusal below is libheif's parse and not a failed open.
        assert!(
            decode::heif_loads(decode::LIBHEIF),
            "{} opens with every entry point",
            decode::LIBHEIF
        );
        assert!(decode::heif_raster(decode::LIBHEIF)(b"\0\0\0\x20ftypavif", (10, 10)).is_err());
    }
}

// Sample: a 2x1 baseline RGB strip, uncompressed, little endian, carrying Orientation (tag 274) in IFD0.
fn oriented_tiff(orientation: u16) -> Vec<u8> {
    let pixels = [255u8, 0, 0, 0, 0, 255];
    let entries: [(u16, u16, u32, u32); 10] = [
        (256, 3, 1, 2),
        (257, 3, 1, 1),
        (258, 3, 3, 8 + 2 + 10 * 12 + 4),
        (259, 3, 1, 1),
        (262, 3, 1, 2),
        (273, 4, 1, 8 + 2 + 10 * 12 + 4 + 6),
        (274, 3, 1, u32::from(orientation)),
        (277, 3, 1, 3),
        (278, 3, 1, 1),
        (279, 4, 1, pixels.len() as u32),
    ];
    let mut b = b"II*\0".to_vec();
    b.extend_from_slice(&8u32.to_le_bytes());
    b.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    for (tag, kind, count, value) in entries {
        b.extend_from_slice(&tag.to_le_bytes());
        b.extend_from_slice(&kind.to_le_bytes());
        b.extend_from_slice(&count.to_le_bytes());
        b.extend_from_slice(&value.to_le_bytes());
    }
    b.extend_from_slice(&0u32.to_le_bytes());
    for _ in 0..3 {
        b.extend_from_slice(&8u16.to_le_bytes());
    }
    b.extend_from_slice(&pixels);
    b
}

#[test]
fn an_oriented_tiff_is_turned_upright_before_it_is_held() {
    let raster = decode::picture_raster();
    let flat = decoded(&raster, &oriented_tiff(1), (2, 1));
    assert_eq!(
        flat.size(0),
        size(DevicePixels(2), DevicePixels(1)),
        "orientation 1 stays 2x1"
    );
    // Orientation 6 turns a quarter clockwise: the upright picture is 1x2, red above blue.
    let turned = decoded(&raster, &oriented_tiff(6), (1, 2));
    assert_eq!(
        turned.size(0),
        size(DevicePixels(1), DevicePixels(2)),
        "orientation 6 is held upright at 1x2"
    );
    let bgra = turned.as_bytes(0).expect("one frame");
    assert_eq!(
        &bgra[..8],
        &[0, 0, 255, 255, 255, 0, 0, 255],
        "red on top, then blue, in BGRA"
    );
}
