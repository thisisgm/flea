//! The scaled codec must keep orientation, colour and formats the old image loader could read.
use super::photo::decode;
use std::path::PathBuf;

struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        let output = std::process::Command::new("mktemp")
            .args(["-d", "/tmp/flea-ql336.XXXXXXXX"])
            .output()
            .unwrap();
        assert!(output.status.success());
        let path = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
        assert!(path.is_absolute() && path.starts_with("/tmp"));
        std::fs::write(path.join(".flea-test-sandbox"), "QL-IMAGE-336\n").unwrap();
        Self(path)
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        assert!(
            self.0.is_absolute()
                && self.0.starts_with("/tmp")
                && self.0.join(".flea-test-sandbox").is_file()
        );
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn gh336_jpeg_physical_fit_applies_exif_turn_and_keeps_bgra_colours() {
    let root = Root::new();
    let picture = image::RgbImage::from_fn(40, 20, |x, _| {
        image::Rgb(if x < 20 { [240, 10, 10] } else { [10, 240, 10] })
    });
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 95)
        .encode_image(&picture)
        .unwrap();
    let exif = b"Exif\0\0II\x2a\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0\x06\0\0\0\0\0\0\0";
    let mut bytes = jpeg[..2].to_vec();
    bytes.extend_from_slice(&[0xff, 0xe1]);
    bytes.extend_from_slice(&((exif.len() + 2) as u16).to_be_bytes());
    bytes.extend_from_slice(exif);
    bytes.extend_from_slice(&jpeg[2..]);
    let path = root.0.join("turned.jpg");
    std::fs::write(&path, bytes).unwrap();
    let decoded = decode(&path, (10, 20), &|| true).unwrap();
    assert_eq!(
        (decoded.size(0).width.0, decoded.size(0).height.0),
        (10, 20)
    );
    let pixels = decoded.as_bytes(0).unwrap();
    assert!(
        pixels[2] > 200 && pixels[1] < 30,
        "the top was the red left half before the EXIF turn"
    );
    let bottom = 19 * 10 * 4;
    assert!(
        pixels[bottom + 1] > 200 && pixels[bottom + 2] < 30,
        "the bottom was the green right half"
    );
}

#[test]
fn gh336_static_fallback_keeps_qoi_and_rejects_renamed_vector() {
    let root = Root::new();
    let path = root.0.join("picture.qoi");
    image::RgbImage::from_pixel(40, 20, image::Rgb([10, 40, 240]))
        .save(&path)
        .unwrap();
    let decoded = decode(&path, (20, 10), &|| true).unwrap();
    assert_eq!(
        (decoded.size(0).width.0, decoded.size(0).height.0),
        (20, 10)
    );
    assert_eq!(&decoded.as_bytes(0).unwrap()[..4], &[240, 40, 10, 255]);
    let disguised = root.0.join("not-png.png");
    std::fs::write(
        &disguised,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100\" height=\"100\"/>",
    )
    .unwrap();
    assert!(decode(&disguised, (20, 20), &|| true).is_err());
    assert!(decode(&path, (8192, 8192), &|| true).is_err());
}

fn high_water_kib() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find_map(|line| {
            line.strip_prefix("VmHWM:")
                .map(|n| n.split_whitespace().next().unwrap().parse().unwrap())
        })
        .unwrap()
}

#[test]
fn gh336_decode_memory_child() {
    let Some(path) = std::env::var_os("FLEA_QL336_MEMORY_INPUT") else {
        return;
    };
    let start = high_water_kib();
    if std::env::var("FLEA_QL336_MEMORY_MODE").unwrap() == "full" {
        let pixels = image::ImageReader::open(&path)
            .unwrap()
            .decode()
            .unwrap()
            .into_rgba8();
        std::hint::black_box(pixels);
    } else {
        let pixels = decode(std::path::Path::new(&path), (765, 574), &|| true).unwrap();
        assert_eq!(
            (pixels.size(0).width.0, pixels.size(0).height.0),
            (765, 574)
        );
        std::hint::black_box(pixels);
    }
    println!(
        "QL336_MEMORY start_kib={start} peak_kib={}",
        high_water_kib()
    );
}

#[test]
fn gh336_physical_fit_avoids_full_photo_allocation_in_fresh_processes() {
    let root = Root::new();
    for extension in ["png", "jpg"] {
        let path = root.0.join(format!("large.{extension}"));
        image::RgbImage::from_pixel(4000, 3000, image::Rgb([40, 100, 200]))
            .save(&path)
            .unwrap();
        let mut peaks = [Vec::new(), Vec::new()];
        for (arm, mode) in ["full", "fit"].iter().enumerate() {
            for _ in 0..2 {
                let output = std::process::Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "g2::pv1::photo_tests::gh336_decode_memory_child",
                        "--nocapture",
                    ])
                    .env("FLEA_QL336_MEMORY_INPUT", &path)
                    .env("FLEA_QL336_MEMORY_MODE", mode)
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                let stdout = String::from_utf8(output.stdout).unwrap();
                let receipt = stdout
                    .lines()
                    .find(|line| line.starts_with("QL336_MEMORY "))
                    .unwrap();
                let numbers: Vec<u64> = receipt
                    .split_whitespace()
                    .skip(1)
                    .map(|v| v.split_once('=').unwrap().1.parse().unwrap())
                    .collect();
                peaks[arm].push(numbers[1].saturating_sub(numbers[0]));
            }
        }
        eprintln!(
            "gh336 {extension} decode peak increase KiB: full={:?} fit={:?}",
            peaks[0], peaks[1]
        );
        let min_full = *peaks[0].iter().min().unwrap();
        let max_fit = *peaks[1].iter().max().unwrap();
        // Fit must never pay the full source. PNG decodes bounded here, so it keeps the tight budget;
        // a JPEG loader that scales after decoding (gdk-pixbuf on some builds) can only promise the floor.
        assert!(
            max_fit + 11_718 < min_full,
            "{extension} still pays a full-source allocation"
        );
        if extension == "png" {
            // Full 4000x3000 RGBA requires 46,875 KiB; fit must stay below half, including codec overhead.
            assert!(max_fit < 23_437, "png fit budget: {max_fit} KiB");
        }
    }
}

#[test]
fn gh336_png_adam7_matches_source_colours_and_reduces_alpha_without_bleed() {
    let root = Root::new();
    // An 8x8 Adam7 fixture: half translucent red, half opaque green.
    let hex = "89504e470d0a1a0a0000000d4948445200000008000000080203000001ce66668e000000206348524d00007a26000080840000fa00000080e8000075300000ea6000003a98000017709cba513c00000009504c5445f0201010d040ffffff9ab4d72f0000000174524e5380ad5e5b4600000001624b474402660b7c640000000774494d4507ea0a19111e28e0bacc92000000154944415408d763606070601000435624c8100a4700165c01e3807b2fd80000002574455874646174653a63726561746500323032362d31302d32355431373a33303a34302b30303a303048bbc5300000002574455874646174653a6d6f6469667900323032362d31302d32355431373a33303a34302b30303a303039e67d8c0000002874455874646174653a74696d657374616d7000323032362d31302d32355431373a33303a34302b30303a30306ef35c530000000049454e44ae426082";
    let bytes: Vec<u8> = hex
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect();
    let path = root.0.join("adam7.png");
    std::fs::write(&path, &bytes).unwrap();
    let expected = image::load_from_memory(&bytes).unwrap().into_rgba8();
    let decoded = decode(&path, (8, 8), &|| true).unwrap();
    for (rgba, bgra) in expected
        .as_raw()
        .as_chunks::<4>()
        .0
        .iter()
        .zip(decoded.as_bytes(0).unwrap().as_chunks::<4>().0.iter())
    {
        assert_eq!(bgra, &[rgba[2], rgba[1], rgba[0], rgba[3]]);
    }
    let fit = decode(&path, (1, 1), &|| true).unwrap();
    let pixel = fit.as_bytes(0).unwrap();
    assert!((190..=193).contains(&pixel[3]), "averaged alpha");
    assert!(
        (89..=92).contains(&pixel[2]) && (148..=151).contains(&pixel[1]),
        "premultiplied colour average"
    );
    assert!(
        decode(&path, (8, 8), &|| false).is_err(),
        "superseded rows stop decoding"
    );
}

#[test]
fn gh336_fallback_scaling_preserves_transparent_edge_colour() {
    let mut pixels = image::RgbaImage::new(2, 1);
    pixels.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
    pixels.put_pixel(1, 0, image::Rgba([0, 0, 0, 0]));
    let root = Root::new();
    for extension in ["gif", "qoi"] {
        let path = root.0.join(format!("edge.{extension}"));
        pixels.save(&path).unwrap();
        let decoded = decode(&path, (1, 1), &|| true).unwrap();
        // Half coverage is 50% alpha, and a codec rounds that to 127 or 128; which one is the loader's.
        let pixel = decoded.as_bytes(0).unwrap();
        assert_eq!(
            (pixel[0], pixel[1], pixel[2]),
            (0, 0, 255),
            "{extension} retains straight colour at half coverage"
        );
        assert!(
            (127..=128).contains(&pixel[3]),
            "{extension} half coverage reads {} not 50%",
            pixel[3]
        );
    }
}

#[test]
fn gh336_png_final_downscale_matches_original_catmull_rom() {
    let root = Root::new();
    let source = image::RgbaImage::from_fn(29, 23, |x, y| {
        image::Rgba([
            ((x * 67 + y * 13) % 256) as u8,
            if x < 13 { 240 } else { 10 },
            ((x * 11 + y * 97) % 256) as u8,
            255,
        ])
    });
    let path = root.0.join("detail.png");
    source.save(&path).unwrap();
    let expected = image::imageops::resize(&source, 11, 9, image::imageops::FilterType::CatmullRom);
    let actual = super::photo::decode_sharp(&path, (11, 9), &|| true).unwrap();
    for (rgba, bgra) in expected
        .pixels()
        .zip(actual.as_bytes(0).unwrap().as_chunks::<4>().0.iter())
    {
        let expected = [rgba[2], rgba[1], rgba[0], rgba[3]];
        assert!(
            expected.iter().zip(bgra).all(|(a, b)| a.abs_diff(*b) <= 1),
            "GM 2026-10-31 12:06 requires a Catmull-Rom or Lanczos3 class final filter: {expected:?} != {bgra:?}"
        );
    }
}

#[test]
fn gh336_png_signed_filter_returns_straight_bgra_for_gpui() {
    let root = Root::new();
    let source = image::RgbaImage::from_raw(
        4,
        1,
        vec![255, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 255, 0, 0, 0, 0],
    )
    .unwrap();
    let path = root.0.join("signed-alpha.png");
    source.save(&path).unwrap();
    let actual = super::photo::decode_sharp(&path, (3, 1), &|| true).unwrap();
    // GPUI accepts straight BGRA and applies alpha in its compositor, including on premultiplied targets.
    assert_eq!(&actual.as_bytes(0).unwrap()[..4], &[0, 0, 255, 177]);
}

#[test]
fn gh336_png_signed_alpha_overshoot_preserves_constant_colour() {
    let root = Root::new();
    let source = image::RgbaImage::from_raw(
        4,
        1,
        vec![128, 0, 0, 0, 128, 0, 0, 255, 128, 0, 0, 255, 128, 0, 0, 255],
    )
    .unwrap();
    let path = root.0.join("alpha-overshoot.png");
    source.save(&path).unwrap();
    let actual = super::photo::decode_sharp(&path, (3, 1), &|| true).unwrap();
    assert_eq!(
        &actual.as_bytes(0).unwrap()[4..8],
        &[0, 0, 128, 255],
        "clipping the encoded alpha must not brighten a constant straight colour"
    );
}
