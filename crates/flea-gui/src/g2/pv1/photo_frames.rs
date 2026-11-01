//! The existing GIF/WebP animation and static-codec fallbacks, with each retained frame sized for the card.
use gpui::RenderImage;
use image::{imageops::FilterType, AnimationDecoder, ImageDecoder, ImageFormat};
use std::{fs::File, io::BufReader, sync::Arc};

pub(super) fn animated(
    file: File,
    format: ImageFormat,
    target: (u32, u32),
    current: &dyn Fn() -> bool,
) -> Result<Option<Arc<RenderImage>>, ()> {
    let frames = match format {
        ImageFormat::Gif => image::codecs::gif::GifDecoder::new(BufReader::new(file))
            .map_err(|_| ())?
            .into_frames(),
        ImageFormat::WebP => {
            let mut decoder =
                image::codecs::webp::WebPDecoder::new(BufReader::new(file)).map_err(|_| ())?;
            if !decoder.has_animation() {
                return Ok(None);
            }
            decoder
                .set_background_color(image::Rgba([0, 0, 0, 0]))
                .map_err(|_| ())?;
            decoder.into_frames()
        }
        _ => return Ok(None),
    };
    let mut scaled = Vec::new();
    for frame in frames {
        if !current() {
            return Err(());
        }
        // GPUI skips failed frames too, preserving the readable part of an animation.
        if let Ok(frame) = frame {
            let delay = frame.delay();
            let buffer = frame.into_buffer();
            let size = within_source(buffer.dimensions(), target);
            let pixels = resized_bgra(buffer, size);
            scaled.push(image::Frame::from_parts(pixels, 0, 0, delay));
        }
    }
    if scaled.is_empty() {
        Err(())
    } else {
        Ok(Some(Arc::new(RenderImage::new(scaled))))
    }
}

pub(super) fn static_image(
    file: File,
    format: ImageFormat,
    target: (u32, u32),
) -> Result<Arc<RenderImage>, ()> {
    let mut decoder = image::ImageReader::with_format(BufReader::new(file), format)
        .into_decoder()
        .map_err(|_| ())?;
    let (sw, sh) = decoder.dimensions();
    if u64::from(sw) * u64::from(sh) > flea::ui_logic_pv1::pixels::MAX_SOURCE_PIXELS {
        return Err(());
    }
    let orientation = decoder.orientation().map_err(|_| ())?;
    let mut decoded = image::DynamicImage::from_decoder(decoder).map_err(|_| ())?;
    decoded.apply_orientation(orientation);
    let size = within_source((decoded.width(), decoded.height()), target);
    let pixels = resized_bgra(decoded.into_rgba8(), size);
    Ok(Arc::new(RenderImage::new(vec![image::Frame::new(pixels)])))
}

/// A raster request inside its source: a smaller source keeps its own pixels, never enlarged.
pub(crate) fn within_source(source: (u32, u32), target: (u32, u32)) -> (u32, u32) {
    if target.0 <= source.0 && target.1 <= source.1 {
        return target;
    }
    let fit = (f64::from(target.0) / f64::from(source.0.max(1)))
        .min(f64::from(target.1) / f64::from(source.1.max(1)))
        .min(1.);
    (
        ((f64::from(source.0) * fit).round() as u32).clamp(1, source.0.max(1)),
        ((f64::from(source.1) * fit).round() as u32).clamp(1, source.1.max(1)),
    )
}

/// Filter premultiplied samples, then restore straight BGRA for the renderer.
pub(crate) fn resized_bgra(mut pixels: image::RgbaImage, target: (u32, u32)) -> image::RgbaImage {
    if pixels.dimensions() == target {
        for pixel in pixels.pixels_mut() {
            pixel.0.swap(0, 2);
        }
        return pixels;
    }
    for pixel in pixels.pixels_mut() {
        let alpha = u16::from(pixel[3]);
        for value in &mut pixel.0[..3] {
            *value = ((u16::from(*value) * alpha + 127) / 255) as u8;
        }
    }
    let mut pixels = image::imageops::resize(&pixels, target.0, target.1, FilterType::CatmullRom);
    for pixel in pixels.pixels_mut() {
        let alpha = u16::from(pixel[3]);
        for value in &mut pixel.0[..3] {
            *value = (u16::from(*value) * 255 + alpha / 2)
                .checked_div(alpha)
                .map_or(0, |straight| straight.min(255) as u8);
        }
        pixel.0.swap(0, 2);
    }
    pixels
}
