//! PNG Catmull-Rom reduction from scanlines, Adam7 included, with no source-sized buffer; bands of output rows reduce on their own threads.
use super::sample::Axis;
use gpui::RenderImage;
use image::metadata::Orientation;
use std::{fs::File, io::BufReader, ops::Range, sync::Arc};

/// One scanline handler: (x0, dx, y) and the line's pixels.
type Scanline<'a> = dyn FnMut((u32, usize, u32), &[u8]) -> Result<(), ()> + 'a;

// At most this many band threads, beside the inflating one; a small target reduces on the inflating thread.
const BANDS: usize = 4;
const BAND_ROWS: u32 = 64;
// Scanlines in flight to one band before the inflating thread waits for it.
const QUEUED: usize = 64;

/// Straight 8-bit pixels as premultiplied BGRA, the order the sums keep.
fn premultiplied(colour: png::ColorType, pixel: &[u8]) -> Option<[f32; 4]> {
    let rgba = match colour {
        png::ColorType::Grayscale => [pixel[0], pixel[0], pixel[0], 255],
        png::ColorType::GrayscaleAlpha => [pixel[0], pixel[0], pixel[0], pixel[1]],
        png::ColorType::Rgb => [pixel[0], pixel[1], pixel[2], 255],
        png::ColorType::Rgba => [pixel[0], pixel[1], pixel[2], pixel[3]],
        png::ColorType::Indexed => return None,
    };
    let alpha = f32::from(rgba[3]);
    Some([
        f32::from(rgba[2]) * alpha / 255.,
        f32::from(rgba[1]) * alpha / 255.,
        f32::from(rgba[0]) * alpha / 255.,
        alpha,
    ])
}

/// One band's output rows and their sums; every scanline adds into them in the order the file stores it.
struct Band<'a> {
    rows: Range<usize>,
    sums: Vec<[f32; 4]>,
    horizontal: Vec<[f32; 4]>,
    xs: &'a Axis,
    ys: &'a Axis,
    colour: png::ColorType,
    w: usize,
}

impl Band<'_> {
    fn add(&mut self, (x0, dx, y): (u32, usize, u32), data: &[u8]) -> Result<(), ()> {
        let channels = self.colour.samples();
        self.horizontal.fill([0.; 4]);
        for (x, pixel) in (x0..).step_by(dx).zip(data.chunks_exact(channels)) {
            let values = premultiplied(self.colour, pixel).ok_or(())?;
            self.xs.each(x, |tap| {
                for (sum, value) in self.horizontal[tap.at].iter_mut().zip(values) {
                    *sum += value * tap.weight;
                }
            });
        }
        let (rows, w) = (self.rows.clone(), self.w);
        self.ys.each(y, |tap| {
            if !rows.contains(&tap.at) {
                return;
            }
            let at = (tap.at - rows.start) * w;
            for (sum, row) in self.sums[at..at + w].iter_mut().zip(&self.horizontal) {
                for (sum, value) in sum.iter_mut().zip(row) {
                    *sum += value * tap.weight;
                }
            }
        });
        Ok(())
    }
}

/// The output rows a scanline adds into, as a range.
fn reach(ys: &Axis, y: u32) -> Range<usize> {
    let (mut first, mut end) = (usize::MAX, 0);
    ys.each(y, |tap| {
        first = first.min(tap.at);
        end = end.max(tap.at + 1);
    });
    first.min(end)..end
}

pub(super) fn decode(
    file: File,
    target: (u32, u32),
    current: &dyn Fn() -> bool,
) -> Result<Arc<RenderImage>, ()> {
    let threads = std::thread::available_parallelism().map_or(1, |count| count.get());
    reduce(file, target, current, BANDS.min(threads.saturating_sub(1)))
}

/// The reduction with at most `most` bands, whatever the core count; one band reduces on the inflating thread.
pub(super) fn reduce(
    file: File,
    target: (u32, u32),
    current: &dyn Fn() -> bool,
    most: usize,
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
    if u64::from(sw) * u64::from(sh) > flea::ui_logic_pv1::pixels::MAX_SOURCE_PIXELS {
        return Err(());
    }
    let orientation = info
        .exif_metadata
        .as_deref()
        .and_then(Orientation::from_exif_chunk)
        .unwrap_or(Orientation::NoTransforms);
    let (w, h) = super::photo_frames::within_source(
        (sw, sh),
        if orientation.to_exif() >= 5 {
            (target.1, target.0)
        } else {
            target
        },
    );
    let (colour, depth) = reader.output_color_type();
    if depth != png::BitDepth::Eight || colour == png::ColorType::Indexed {
        return Err(());
    }
    let channels = colour.samples();
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
    let (xs, ys) = (Axis::new(sw, w), Axis::new(sh, h));
    let count = (h / BAND_ROWS).clamp(1, most.max(1) as u32) as usize;
    let mut bands: Vec<Band> = (0..count)
        .map(|band| {
            let rows = band * h as usize / count..(band + 1) * h as usize / count;
            Band {
                sums: vec![[0.; 4]; rows.len() * w as usize],
                horizontal: vec![[0.; 4]; w as usize],
                rows,
                xs: &xs,
                ys: &ys,
                colour,
                w: w as usize,
            }
        })
        .collect();
    // Each scanline in file order: (x0, dx, y) and its pixels, checked against the pass's own width.
    let scan = |reader: &mut png::Reader<BufReader<File>>,
                each: &mut Scanline<'_>|
     -> Result<(), ()> {
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
                each((x0, dx, y), row.data())?;
            }
        }
        if reader.next_row().map_err(|_| ())?.is_some() {
            return Err(());
        }
        Ok(())
    };
    if count == 1 {
        let band = &mut bands[0];
        scan(&mut reader, &mut |at, data| band.add(at, data))?;
    } else {
        std::thread::scope(|scope| -> Result<(), ()> {
            let mut queues = Vec::with_capacity(count);
            let mut workers = Vec::with_capacity(count);
            for mut band in bands.drain(..) {
                let (send, receive) =
                    std::sync::mpsc::sync_channel::<((u32, usize, u32), Arc<[u8]>)>(QUEUED);
                queues.push((band.rows.clone(), send));
                workers.push(scope.spawn(move || {
                    let mut failed = false;
                    for (at, data) in receive {
                        failed = failed || band.add(at, &data).is_err();
                    }
                    (!failed).then_some(band).ok_or(())
                }));
            }
            let read = scan(&mut reader, &mut |at, data| {
                let (data, reach) = (Arc::<[u8]>::from(data), reach(&ys, at.2));
                for (rows, queue) in &queues {
                    if rows.start < reach.end && reach.start < rows.end {
                        queue.send((at, data.clone())).map_err(|_| ())?;
                    }
                }
                Ok(())
            });
            drop(queues);
            for worker in workers {
                bands.push(worker.join().map_err(|_| ())??);
            }
            read
        })?;
    }
    let mut bytes = Vec::with_capacity(w as usize * h as usize * 4);
    for sum in bands.iter().flat_map(|band| &band.sums) {
        let alpha = sum[3];
        for value in &sum[..3] {
            bytes.push(if alpha > 0. {
                (value * 255. / alpha).round().clamp(0., 255.) as u8
            } else {
                0
            });
        }
        bytes.push(alpha.round().clamp(0., 255.) as u8);
    }
    let pixels = image::RgbaImage::from_raw(w, h, bytes).ok_or(())?;
    let mut upright = image::DynamicImage::ImageRgba8(pixels);
    upright.apply_orientation(orientation);
    Ok(Arc::new(RenderImage::new(vec![image::Frame::new(
        upright.into_rgba8(),
    )])))
}
