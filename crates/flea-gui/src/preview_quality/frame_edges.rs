//! Keep the thumbnailer's movie framing while replacing its picture with source pixels.
use gpui::RenderImage;
use image::{
    imageops::{resize, FilterType},
    RgbaImage,
};

// A seed smaller than this on either edge has no border worth keeping.
const MIN_SEED: u32 = 16;
// The widest border scanned, in seed pixels; a quarter of the seed's width caps it first.
const MAX_STRIP: u32 = 256;
// Grey levels counted as a perforation's light and as the strip's dark.
const LIGHT: u8 = 200;
const DARK: u8 = 40;
// The border reads as film only with at least height / SHARE light pixels and as many dark ones.
const SHARE: u32 = 8;

pub(super) fn preserve(pixels: &mut RgbaImage, seed: &RenderImage) {
    let size = seed.size(0);
    let (w, h) = (size.width.0 as u32, size.height.0 as u32);
    let Some(bytes) = seed.as_bytes(0) else {
        return;
    };
    if w < MIN_SEED || h < MIN_SEED {
        return;
    }
    let at = |x: u32, y: u32| &bytes[((y * w + x) * 4) as usize..((y * w + x) * 4 + 3) as usize];
    let limit = (w / 4).min(MAX_STRIP);
    let (mut width, mut light, mut dark) = (0, 0, 0);
    for x in 0..limit {
        if !(0..h).all(|y| {
            let (left, right) = (at(x, y), at(w - 1 - x, y));
            left == right && left[0] == left[1] && left[1] == left[2]
        }) {
            break;
        }
        width += 1;
        for y in 0..h {
            let value = at(x, y)[0];
            light += u32::from(value > LIGHT);
            dark += u32::from(value < DARK);
        }
    }
    if width < 2 || width == limit || light < h / SHARE || dark < h / SHARE {
        return;
    }
    let edge =
        ((width as f64 * pixels.height() as f64 / h as f64).ceil() as u32).min(pixels.width() / 2);
    if edge == 0 {
        return;
    }
    for (from, to) in [(0, 0), (w - width, pixels.width() - edge)] {
        let mut strip_bytes = Vec::with_capacity((width * h * 4) as usize);
        for y in 0..h {
            let start = ((y * w + from) * 4) as usize;
            strip_bytes.extend_from_slice(&bytes[start..start + (width * 4) as usize]);
        }
        let Some(strip) = RgbaImage::from_raw(width, h, strip_bytes) else {
            return;
        };
        let strip = resize(&strip, edge, pixels.height(), FilterType::CatmullRom);
        image::imageops::replace(pixels, &strip, i64::from(to), 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    #[test]
    fn preview_quality_movie_framing_keeps_its_size_and_sharp_content() {
        let mut seed = RgbaImage::from_pixel(256, 144, image::Rgba([10, 20, 30, 255]));
        for y in 0..144 {
            for x in [
                0, 1, 2, 3, 4, 5, 6, 7, 248, 249, 250, 251, 252, 253, 254, 255,
            ] {
                let light = (4..12).contains(&(y % 16)) && (2..6).contains(&(x % 8));
                seed.put_pixel(
                    x,
                    y,
                    image::Rgba([
                        if light { 255 } else { 0 },
                        if light { 255 } else { 0 },
                        if light { 255 } else { 0 },
                        255,
                    ]),
                );
            }
        }
        let seed = Arc::new(RenderImage::new(vec![image::Frame::new(seed)]));
        let mut full = RgbaImage::from_pixel(512, 288, image::Rgba([80, 90, 100, 255]));
        preserve(&mut full, &seed);
        assert_eq!(full.get_pixel(0, 60).0, [0, 0, 0, 255]);
        assert_eq!(
            full.get_pixel(16, 60).0,
            [80, 90, 100, 255],
            "source content is untouched"
        );
        assert_eq!(full.get_pixel(256, 144).0, [80, 90, 100, 255]);
        assert!(
            full.get_pixel(7, 14).0[0] > 200,
            "the movie perforation stays visible"
        );
        let grown_seed = RenderImage::new(vec![image::Frame::new(full)]);
        let mut grown = RgbaImage::from_pixel(1024, 576, image::Rgba([80, 90, 100, 255]));
        preserve(&mut grown, &grown_seed);
        assert_eq!(grown.get_pixel(0, 60).0, [0, 0, 0, 255]);
        assert_eq!(grown.get_pixel(32, 60).0, [80, 90, 100, 255]);
    }
    #[test]
    fn preview_quality_unframed_video_never_copies_thumbnail_pixels() {
        let seed = RenderImage::new(vec![image::Frame::new(RgbaImage::from_pixel(
            256,
            144,
            image::Rgba([10, 20, 30, 255]),
        ))]);
        let mut full = RgbaImage::from_pixel(512, 288, image::Rgba([80, 90, 100, 255]));
        preserve(&mut full, &seed);
        assert!(full.pixels().all(|pixel| pixel.0 == [80, 90, 100, 255]));
    }
}
