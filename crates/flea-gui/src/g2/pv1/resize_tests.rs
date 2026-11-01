//! The streamed reduction is image's Catmull-Rom, byte for byte, for every channel layout it reads.
use super::{reduce, reduce_in};
use image::{imageops, Rgba, RgbaImage};

fn noise(w: u32, h: u32, seed: u32) -> RgbaImage {
    let mut state = seed.wrapping_mul(2_654_435_761) | 1;
    RgbaImage::from_fn(w, h, |_, _| {
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state >> 24) as u8
        };
        Rgba([next(), next(), next(), next()])
    })
}

const SIZES: [((u32, u32), (u32, u32)); 7] = [
    ((97, 61), (13, 9)),
    ((640, 427), (256, 171)),
    ((600, 400), (410, 273)),
    ((300, 200), (300, 37)),
    ((300, 200), (41, 200)),
    ((1201, 799), (902, 601)),
    ((50, 50), (49, 1)),
];

#[test]
fn preview_quality_reduce_matches_image_catmull_rom_exactly() {
    for (index, (source, target)) in SIZES.into_iter().enumerate() {
        let image = noise(source.0, source.1, index as u32 + 1);
        let want = imageops::resize(&image, target.0, target.1, imageops::FilterType::CatmullRom);
        let got = reduce::<4>(image.as_raw(), source, target, [0, 1, 2, 3]);
        assert_eq!(got, want.into_raw(), "{source:?} to {target:?}");
    }
}

#[test]
fn preview_quality_reduce_reads_opaque_rgb_and_grey_as_their_rgba_copy() {
    for (index, (source, target)) in SIZES.into_iter().enumerate() {
        let rgba = noise(source.0, source.1, index as u32 + 11);
        let rgb = image::DynamicImage::ImageRgba8(rgba).into_rgb8();
        let want = imageops::resize(
            &image::DynamicImage::ImageRgb8(rgb.clone()).into_rgba8(),
            target.0,
            target.1,
            imageops::FilterType::CatmullRom,
        );
        let got = reduce::<3>(rgb.as_raw(), source, target, [2, 1, 0, 3]);
        let bgra: Vec<u8> = want
            .pixels()
            .flat_map(|p| [p[2], p[1], p[0], p[3]])
            .collect();
        assert_eq!(got, bgra, "rgb {source:?} to {target:?}");
        let grey = image::DynamicImage::ImageRgb8(rgb).into_luma8();
        let want = imageops::resize(
            &image::DynamicImage::ImageLuma8(grey.clone()).into_rgba8(),
            target.0,
            target.1,
            imageops::FilterType::CatmullRom,
        );
        let got = reduce::<1>(grey.as_raw(), source, target, [0, 1, 2, 3]);
        assert_eq!(got, want.into_raw(), "grey {source:?} to {target:?}");
    }
}

#[test]
fn preview_quality_reduce_bands_sum_every_row_as_one_pass() {
    for (index, (source, target)) in SIZES.into_iter().enumerate() {
        let image = noise(source.0, source.1, index as u32 + 21);
        let want = imageops::resize(&image, target.0, target.1, imageops::FilterType::CatmullRom);
        let want = want.into_raw();
        for bands in 1..=5 {
            let got = reduce_in::<4>(image.as_raw(), source, target, [0, 1, 2, 3], bands);
            assert_eq!(got, want, "{source:?} to {target:?} in {bands} bands");
        }
    }
}
