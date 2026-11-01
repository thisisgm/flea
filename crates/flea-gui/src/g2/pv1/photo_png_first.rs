//! PNG area reduction from scanlines, including Adam7 passes, without a source-sized pixel buffer.
use gpui::RenderImage;
use image::metadata::Orientation;
use std::{fs::File, io::BufReader, sync::Arc};

pub(super) fn decode(
    file: File,
    target: (u32, u32),
    current: &dyn Fn() -> bool,
) -> Result<Arc<RenderImage>, ()> {
    let mut decoder = png::Decoder::new_with_limits(
        BufReader::new(file),
        png::Limits {
            bytes: 64 * 1024 * 1024,
        },
    );
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|_| ())?;
    let info = reader.info();
    let (sw, sh, interlaced) = (info.width, info.height, info.interlaced);
    let orientation = info
        .exif_metadata
        .as_deref()
        .and_then(Orientation::from_exif_chunk)
        .unwrap_or(Orientation::NoTransforms);
    let (w, h) = if orientation.to_exif() >= 5 {
        (target.1, target.0)
    } else {
        target
    };
    let (colour, depth) = reader.output_color_type();
    if depth != png::BitDepth::Eight {
        return Err(());
    }
    let channels = colour.samples();
    let mut sums = vec![[0f32; 4]; w as usize * h as usize];
    let plain = [(0, 0, 1, 1)];
    let adam7 = [
        (0, 0, 8, 8),
        (4, 0, 8, 8),
        (0, 4, 4, 8),
        (2, 0, 4, 4),
        (0, 2, 2, 4),
        (1, 0, 2, 2),
        (0, 1, 1, 2),
    ];
    let passes = if interlaced { &adam7[..] } else { &plain[..] };
    let (xs, ys) = (f64::from(w) / f64::from(sw), f64::from(h) / f64::from(sh));
    for &(x0, y0, dx, dy) in passes {
        if x0 >= sw || y0 >= sh {
            continue;
        }
        for y in (y0..sh).step_by(dy) {
            if !current() {
                return Err(());
            }
            let row = reader.next_row().map_err(|_| ())?.ok_or(())?;
            let count = (sw - x0).div_ceil(dx as u32) as usize;
            if row.data().len() != count * channels {
                return Err(());
            }
            for (x, pixel) in (x0..sw).step_by(dx).zip(row.data().chunks_exact(channels)) {
                let rgba = match colour {
                    png::ColorType::Grayscale => [pixel[0], pixel[0], pixel[0], 255],
                    png::ColorType::GrayscaleAlpha => [pixel[0], pixel[0], pixel[0], pixel[1]],
                    png::ColorType::Rgb => [pixel[0], pixel[1], pixel[2], 255],
                    png::ColorType::Rgba => [pixel[0], pixel[1], pixel[2], pixel[3]],
                    png::ColorType::Indexed => return Err(()),
                };
                let (left, right) = (f64::from(x) * xs, f64::from(x + 1) * xs);
                let (top, bottom) = (f64::from(y) * ys, f64::from(y + 1) * ys);
                let alpha = f32::from(rgba[3]);
                let values = [
                    f32::from(rgba[2]) * alpha / 255.,
                    f32::from(rgba[1]) * alpha / 255.,
                    f32::from(rgba[0]) * alpha / 255.,
                    alpha,
                ];
                for oy in top.floor() as u32..(bottom.ceil() as u32).min(h) {
                    let vy = (bottom.min(f64::from(oy + 1)) - top.max(f64::from(oy))) as f32;
                    for ox in left.floor() as u32..(right.ceil() as u32).min(w) {
                        let vx = (right.min(f64::from(ox + 1)) - left.max(f64::from(ox))) as f32;
                        let sum = &mut sums[(oy * w + ox) as usize];
                        for channel in 0..4 {
                            sum[channel] += values[channel] * vx * vy;
                        }
                    }
                }
            }
        }
    }
    if reader.next_row().map_err(|_| ())?.is_some() {
        return Err(());
    }
    let mut bytes = Vec::with_capacity(w as usize * h as usize * 4);
    for sum in sums {
        let alpha = sum[3].clamp(0., 255.);
        for value in &sum[..3] {
            bytes.push(if alpha > 0. {
                (value * 255. / alpha).round().clamp(0., 255.) as u8
            } else {
                0
            });
        }
        bytes.push(alpha.round() as u8);
    }
    let pixels = image::RgbaImage::from_raw(w, h, bytes).ok_or(())?;
    let mut upright = image::DynamicImage::ImageRgba8(pixels);
    upright.apply_orientation(orientation);
    Ok(Arc::new(RenderImage::new(vec![image::Frame::new(
        upright.into_rgba8(),
    )])))
}
