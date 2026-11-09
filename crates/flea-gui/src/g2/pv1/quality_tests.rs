//! Physical preview requests and sharp, alpha-correct reduction through the actual codecs.
use super::photo_frames;
use image::{imageops::FilterType, Rgba, RgbaImage};

fn specimen() -> RgbaImage {
    RgbaImage::from_fn(96, 64, |x, y| {
        let v = if (x / 3 + y / 5) % 2 == 0 { 240 } else { 20 };
        Rgba([v, v / 2, 255 - v, 255])
    })
}

fn reference(source: &RgbaImage, target: (u32, u32)) -> RgbaImage {
    let mut expected = image::imageops::resize(source, target.0, target.1, FilterType::CatmullRom);
    for pixel in expected.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    expected
}

#[test]
fn preview_quality_fallback_preserves_catmull_rom_detail() {
    let source = specimen();
    let expected = reference(&source, (37, 25));
    let actual = photo_frames::resized_bgra(source, (37, 25));
    assert!(
        actual == expected,
        "fallback must preserve Catmull-Rom detail"
    );
}

#[test]
fn preview_quality_small_sources_decode_at_their_own_pixels() {
    let root = Root::new();
    let art = RgbaImage::from_fn(64, 64, |x, y| {
        Rgba(if (x + y) % 2 == 0 {
            [0, 0, 0, 255]
        } else {
            [255, 255, 255, 255]
        })
    });
    let photo = RgbaImage::from_fn(320, 240, |x, y| {
        Rgba([x as u8, y as u8, (x ^ y) as u8, 255])
    });
    // PNG takes the scanline sampler, TIFF the whole-image codec; neither may enlarge.
    for (name, source) in [
        ("art.png", &art),
        ("photo.png", &photo),
        ("art.tiff", &art),
        ("photo.tiff", &photo),
    ] {
        let path = root.0.join(name);
        source.save(&path).unwrap();
        let (w, h) = source.dimensions();
        for request in [(w * 5, h * 5), (w + 1, h + 1), (w * 2, h)] {
            let image = super::photo::decode_sharp(&path, request, &|| true).unwrap();
            let size = image.size(0);
            assert_eq!(
                (size.width.0 as u32, size.height.0 as u32),
                (w, h),
                "{name} asked for {request:?}"
            );
            let bgra = image.as_bytes(0).unwrap();
            assert!(
                bgra.as_chunks::<4>()
                    .0
                    .iter()
                    .copied()
                    .zip(source.pixels())
                    .all(|(drawn, own)| drawn == [own[2], own[1], own[0], own[3]]),
                "{name} keeps every source pixel"
            );
        }
    }
    assert_eq!(
        photo_frames::within_source((320, 240), (400, 200)),
        (267, 200)
    );
}

#[test]
fn preview_quality_filter_never_bleeds_transparent_colour() {
    let source = RgbaImage::from_fn(96, 64, |x, _| {
        if x < 48 {
            Rgba([0, 255, 0, 255])
        } else {
            Rgba([255, 0, 0, 0])
        }
    });
    let held = photo_frames::resized_bgra(source, (37, 25));
    assert!(held.pixels().all(|p| p[2] == 0));
    assert!(held.pixels().filter(|p| p[3] > 16).all(|p| p[1] >= 250));
}

#[test]
fn preview_quality_png_matches_sharp_reference() {
    let root = Root::new();
    let source = specimen();
    let path = root.0.join("sharp.png");
    source.save(&path).unwrap();
    let image =
        super::photo_png::decode(std::fs::File::open(&path).unwrap(), (37, 25), &|| true).unwrap();
    let expected = reference(&source, (37, 25));
    let errors: Vec<_> = image
        .as_bytes(0)
        .unwrap()
        .iter()
        .zip(expected.as_raw())
        .map(|(&a, &b)| a.abs_diff(b))
        .collect();
    assert!(
        errors.iter().all(|&error| error <= 1),
        "largest channel error {:?}",
        errors.iter().max()
    );
}

#[test]
fn preview_quality_mixed_axis_resize_still_filters() {
    let source = specimen();
    let expected = reference(&source, (97, 63));
    let actual = photo_frames::resized_bgra(source, (97, 63));
    assert!(
        actual == expected,
        "a reduction on one axis keeps Catmull-Rom on both"
    );
}

// The png encoder writes only plain rows, so the Adam7 rows are laid out here and stored uncompressed.
fn adam7_png(source: &RgbaImage) -> Vec<u8> {
    const ADLER_MODULUS: u32 = 65521;
    let (w, h) = source.dimensions();
    let mut raw = Vec::new();
    let passes = [
        (0, 0, 8, 8),
        (4, 0, 8, 8),
        (0, 4, 4, 8),
        (2, 0, 4, 4),
        (0, 2, 2, 4),
        (1, 0, 2, 2),
        (0, 1, 1, 2),
    ];
    for (x0, y0, dx, dy) in passes {
        if x0 >= w || y0 >= h {
            continue;
        }
        for y in (y0..h).step_by(dy) {
            raw.push(0);
            for x in (x0..w).step_by(dx) {
                raw.extend_from_slice(&source.get_pixel(x, y).0);
            }
        }
    }
    // A zlib header, stored deflate blocks of at most 65535 bytes, then the Adler-32 of the rows.
    let mut zlib = vec![0x78, 0x01];
    let blocks: Vec<_> = raw.chunks(usize::from(u16::MAX)).collect();
    for (at, block) in blocks.iter().enumerate() {
        let len = block.len() as u16;
        zlib.push(u8::from(at + 1 == blocks.len()));
        zlib.extend_from_slice(&len.to_le_bytes());
        zlib.extend_from_slice(&(!len).to_le_bytes());
        zlib.extend_from_slice(block);
    }
    let (a, b) = raw.iter().fold((1u32, 0u32), |(a, b), &byte| {
        let a = (a + u32::from(byte)) % ADLER_MODULUS;
        (a, (b + a) % ADLER_MODULUS)
    });
    zlib.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut info = png::Info::with_size(w, h);
    info.color_type = png::ColorType::Rgba;
    info.bit_depth = png::BitDepth::Eight;
    info.interlaced = true;
    let mut bytes = Vec::new();
    let mut writer = png::Encoder::with_info(&mut bytes, info)
        .unwrap()
        .write_header()
        .unwrap();
    writer.write_chunk(png::chunk::IDAT, &zlib).unwrap();
    writer.finish().unwrap();
    bytes
}

#[test]
fn preview_quality_png_adam7_matches_plain_rows() {
    let root = Root::new();
    for (source, target) in [
        (specimen(), (37, 25)),
        (
            RgbaImage::from_fn(5, 3, |x, y| Rgba([x as u8 * 50, y as u8 * 90, 7, 255])),
            (3, 2),
        ),
    ] {
        let (plain, laced) = (root.0.join("plain.png"), root.0.join("laced.png"));
        source.save(&plain).unwrap();
        std::fs::write(&laced, adam7_png(&source)).unwrap();
        let decode = |path: &std::path::Path| {
            super::photo_png::decode(std::fs::File::open(path).unwrap(), target, &|| true).unwrap()
        };
        let (plain, laced) = (decode(&plain), decode(&laced));
        let errors: Vec<_> = plain
            .as_bytes(0)
            .unwrap()
            .iter()
            .zip(laced.as_bytes(0).unwrap())
            .map(|(&a, &b)| a.abs_diff(b))
            .collect();
        assert!(
            errors.len() == (target.0 * target.1 * 4) as usize
                && errors.iter().all(|&error| error <= 1),
            "Adam7 passes reduce like plain rows, largest error {:?}",
            errors.iter().max()
        );
    }
}

#[test]
fn preview_quality_alpha_edges_never_bleed_or_wrap() {
    let root = Root::new();
    let path = root.0.join("edge.png");
    RgbaImage::from_fn(96, 64, |x, _| {
        if x < 48 {
            Rgba([255, 0, 0, 0])
        } else {
            Rgba([0, 0, 255, 255])
        }
    })
    .save(&path)
    .unwrap();
    let image =
        super::photo_png::decode(std::fs::File::open(&path).unwrap(), (37, 25), &|| true).unwrap();
    let pixels = image
        .as_bytes(0)
        .unwrap()
        .as_chunks::<4>()
        .0
        .iter()
        .copied();
    assert!(
        pixels.clone().all(|p| p[2] <= 1),
        "transparent red never bleeds"
    );
    assert!(pixels.filter(|p| p[3] > 16).all(|p| p[0] >= 250));
    // Opaque black under the negative lobe leaves more colour than alpha; it must clamp to white, never wrap.
    let source = RgbaImage::from_fn(16, 1, |x, _| {
        if x < 8 {
            Rgba([0, 0, 0, 255])
        } else {
            Rgba([255, 255, 255, 50])
        }
    });
    let held = photo_frames::resized_bgra(source, (5, 1));
    let edge = held.get_pixel(3, 0);
    assert!(edge[3] < 50 && edge.0[..3] == [255; 3], "{edge:?}");
}

#[test]
fn preview_quality_raster_requests_cover_fractional_device_rects() {
    for scale in [1., 1.25, 1.5, 2.] {
        let requested =
            flea::ui_logic_pv1::pixels::decode_size(301., 199., scale, 6000., 4000., false);
        assert!(f64::from(requested.0) >= 301. * scale);
        assert!(f64::from(requested.1) >= 199. * scale);
    }
}

struct Root(std::path::PathBuf);
impl Root {
    fn new() -> Self {
        let parent = std::env::temp_dir();
        let output = std::process::Command::new("mktemp")
            .arg("-d")
            .arg(parent.join("flea-preview-quality.XXXXXXXX"))
            .output()
            .unwrap();
        assert!(output.status.success());
        let path = std::path::PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
        assert!(path.is_absolute() && path.starts_with(&parent));
        std::fs::write(path.join(".flea-test-sandbox"), "PREVIEW-QUALITY").unwrap();
        Self(path)
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        assert!(
            self.0.is_absolute()
                && self.0.starts_with(std::env::temp_dir())
                && self.0.join(".flea-test-sandbox").is_file()
        );
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn b03_png_bands_reduce_exactly_like_one_band() {
    let root = Root::new();
    let source = RgbaImage::from_fn(640, 420, |x, y| {
        Rgba([
            (x * 7 + y * 3) as u8,
            (x ^ y) as u8,
            (y * 11) as u8,
            if (x / 9 + y / 13) % 5 == 0 {
                0
            } else {
                40 + (x % 200) as u8
            },
        ])
    });
    let (plain, laced) = (root.0.join("bands.png"), root.0.join("bands-laced.png"));
    source.save(&plain).unwrap();
    std::fs::write(&laced, adam7_png(&source)).unwrap();
    for path in [&plain, &laced] {
        for target in [(320, 210), (213, 140), (640, 420)] {
            let reduce = |most| {
                super::photo_png::reduce(std::fs::File::open(path).unwrap(), target, &|| true, most)
                    .unwrap()
            };
            let (one, banded) = (reduce(1), reduce(4));
            assert_eq!(banded.size(0), one.size(0));
            assert!(
                banded.as_bytes(0) == one.as_bytes(0),
                "{} at {target:?}: bands add every scanline in file order",
                path.display()
            );
        }
    }
}
