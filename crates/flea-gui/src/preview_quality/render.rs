//! Refinement uses the same codecs and sandboxes as existing previews.
use super::Request;
use crate::g2::{pv1, pv2::poppler};
use gpui::{RenderImage, SvgRenderer};
use std::sync::{atomic::Ordering, Arc};

pub(super) fn run(request: &Request, renderer: SvgRenderer) -> Result<Arc<RenderImage>, ()> {
    let current = || request.current.load(Ordering::Acquire);
    if !current() || request.size.0 == 0 || request.size.1 == 0 {
        return Err(());
    }
    let name = request
        .path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(())?;
    let ext = request
        .path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if flea::ui_logic_pv1::pixels::is_host_decoded(name) {
        let job = pv1::raster::Job {
            path: request.path.clone(),
            size: request.size,
            epoch: 1,
            latest: Arc::new(std::sync::atomic::AtomicU64::new(1)),
            raster: pv1::raster::for_path(&request.path, renderer),
        };
        return match pv1::raster::run(&job) {
            pv1::raster::RasterOutcome::Raster(image) if current() => Ok(image),
            _ => Err(()),
        };
    }
    if ext == "pdf" {
        let doc = poppler::Document::open(&request.path).map_err(|_| ())?;
        let points = doc.page_size(0).ok_or(())?;
        let ratio = (request.size.0 as f64 / points.0).min(request.size.1 as f64 / points.1);
        let raster = doc.render(0, (points.0 * ratio).ceil() as u32).ok_or(())?;
        let pixels =
            image::RgbaImage::from_raw(raster.width, raster.height, raster.bgra).ok_or(())?;
        return Ok(Arc::new(RenderImage::new(vec![image::Frame::new(pixels)])));
    }
    if let Ok(image) = pv1::photo::decode_sharp(&request.path, request.size, &current) {
        return Ok(image);
    }
    if !current() {
        return Err(());
    }
    let bytes = super::helper::run(request)?;
    if !current() {
        return Err(());
    }
    decoded_thumbnail(
        &bytes,
        request.size,
        request.seed.lock().ok().and_then(|mut seed| seed.take()),
    )
}

fn decoded_thumbnail(
    bytes: &[u8],
    target: (u32, u32),
    seed: Option<Arc<RenderImage>>,
) -> Result<Arc<RenderImage>, ()> {
    use image::ImageDecoder;
    let decoder = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| ())?
        .into_decoder()
        .map_err(|_| ())?;
    let (w, h) = decoder.dimensions();
    if u64::from(w) * u64::from(h) > flea::ui_logic_pv1::pixels::MAX_SOURCE_PIXELS {
        return Err(());
    }
    let source = image::DynamicImage::from_decoder(decoder)
        .map_err(|_| ())?
        .into_rgba8();
    let ratio = (target.0 as f64 / w as f64).min(target.1 as f64 / h as f64);
    // A video frame fills its box; any other raster keeps its own size.
    let ratio = if seed.is_some() { ratio } else { ratio.min(1.) };
    let size = (
        (w as f64 * ratio).ceil() as u32,
        (h as f64 * ratio).ceil() as u32,
    );
    let mut pixels = pv1::photo_frames::resized_bgra(source, size);
    if let Some(seed) = seed {
        super::frame_edges::preserve(&mut pixels, &seed);
    }
    Ok(Arc::new(RenderImage::new(vec![image::Frame::new(pixels)])))
}
