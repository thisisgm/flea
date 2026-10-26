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
            let pixels = resized_bgra(frame.into_buffer(), target);
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
    let orientation = decoder.orientation().map_err(|_| ())?;
    let mut decoded = image::DynamicImage::from_decoder(decoder).map_err(|_| ())?;
    decoded.apply_orientation(orientation);
    let pixels = resized_bgra(decoded.into_rgba8(), target);
    Ok(Arc::new(RenderImage::new(vec![image::Frame::new(pixels)])))
}

/// Filter premultiplied samples, then restore straight BGRA for the renderer.
pub(super) fn resized_bgra(mut pixels: image::RgbaImage, target: (u32, u32)) -> image::RgbaImage {
    for pixel in pixels.pixels_mut() {
        let alpha = u16::from(pixel[3]);
        for value in &mut pixel.0[..3] {
            *value = ((u16::from(*value) * alpha + 127) / 255) as u8;
        }
    }
    let mut pixels = image::imageops::resize(&pixels, target.0, target.1, FilterType::Triangle);
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
