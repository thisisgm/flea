//! The existing GIF/WebP animation and static-codec fallbacks, with each retained frame sized for the card.
use gpui::RenderImage;
use image::{imageops::FilterType, AnimationDecoder, ImageDecoder, ImageFormat};
use std::{fs::File, io::BufReader, path::Path, sync::Arc};

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
    path: &Path,
    file: File,
    format: ImageFormat,
    target: (u32, u32),
    current: &dyn Fn() -> bool,
) -> Result<Arc<RenderImage>, ()> {
    let (source, wanted, _held) = if format == ImageFormat::Jpeg {
        let (pixels, wanted, held) =
            super::photo_source::shared(path, file, current, |file| decode(path, file, format))?;
        (pixels, wanted, Some(held))
    } else {
        (Arc::new(decode(path, file, format)?), false, None)
    };
    // A superseded request still draws pixels nobody else is drawing: the decode is paid, and going back reuses them.
    if !current() && wanted {
        return Err(());
    }
    let orientation = source.1;
    // Reduce in stored orientation and turn only the result, so no source-sized copy is made for either.
    let stored = match orientation.to_exif() {
        5..=8 => (target.1, target.0),
        _ => target,
    };
    let stored = within_source((source.0.width(), source.0.height()), stored);
    let pixels = match opaque_bgra(&source.0, stored) {
        Some(pixels) => pixels,
        None => resized_bgra(
            Arc::try_unwrap(source)
                .map_or_else(|shared| shared.0.to_rgba8(), |own| own.0.into_rgba8()),
            stored,
        ),
    };
    let mut upright = image::DynamicImage::ImageRgba8(pixels);
    upright.apply_orientation(orientation);
    Ok(Arc::new(RenderImage::new(vec![image::Frame::new(
        upright.into_rgba8(),
    )])))
}

/// One bounded read of a source file's pixels, counted for the decodes-per-open reader.
fn decode(
    path: &Path,
    file: File,
    format: ImageFormat,
) -> Result<super::photo_source::Decoded, ()> {
    let mut decoder = image::ImageReader::with_format(BufReader::new(file), format)
        .into_decoder()
        .map_err(|_| ())?;
    let (sw, sh) = decoder.dimensions();
    if u64::from(sw) * u64::from(sh) > flea::ui_logic_pv1::pixels::MAX_SOURCE_PIXELS {
        return Err(());
    }
    let orientation = decoder.orientation().map_err(|_| ())?;
    crate::preview_quality::reuse::decoded(path);
    Ok((
        image::DynamicImage::from_decoder(decoder).map_err(|_| ())?,
        orientation,
    ))
}

/// An opaque 8-bit source reduces straight to BGRA, with no RGBA copy and no alpha passes.
fn opaque_bgra(decoded: &image::DynamicImage, target: (u32, u32)) -> Option<image::RgbaImage> {
    let size = (decoded.width(), decoded.height());
    if target.0 > size.0 || target.1 > size.1 || target == size {
        return None;
    }
    let bgra = match decoded {
        image::DynamicImage::ImageRgb8(rgb) => {
            super::resize::reduce::<3>(rgb.as_raw(), size, target, [2, 1, 0, 3])
        }
        image::DynamicImage::ImageLuma8(grey) => {
            super::resize::reduce::<1>(grey.as_raw(), size, target, [2, 1, 0, 3])
        }
        _ => return None,
    };
    image::RgbaImage::from_raw(target.0, target.1, bgra)
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
    let mut pixels = if target.0 > pixels.width() || target.1 > pixels.height() {
        image::imageops::resize(&pixels, target.0, target.1, FilterType::CatmullRom)
    } else {
        let bytes =
            super::resize::reduce::<4>(pixels.as_raw(), pixels.dimensions(), target, [0, 1, 2, 3]);
        image::RgbaImage::from_raw(target.0, target.1, bytes).unwrap()
    };
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
