//! One bounded job rasterizes SVG or decodes images through their own format decoder off the UI thread (#336, SW-13).
use flea::ui_logic_pv1::pixels::{
    decode_byte_bound, is_compressed_vector, is_heif_name, is_vector_name, svg_within_byte_bound,
    within_decode_bound, MAX_SVG_BYTES,
};
use gpui::{size, DevicePixels, RenderImage, SvgRenderer, SvgSize};
use std::{
    io::Read,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

/// Bytes at a request size in, one raster out; the app's renderer is the default, tests inject their own.
pub type Rasterizer =
    Arc<dyn Fn(&[u8], (u32, u32)) -> Result<Arc<RenderImage>, String> + Send + Sync>;

/// Why a vector file produced no raster; the card draws the matching can't-show sentence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RasterRefusal {
    TooLarge,
    Unreadable,
}

/// A picture's raster as its host holds it (#336): keyed by the device size it was made for.
#[derive(Clone, Debug, Default)]
pub struct RasterState {
    /// The request's device pixels; None until a raster is asked for.
    pub size: Option<(u32, u32)>,
    /// The raster at that size, or None while pending or after a refusal.
    pub image: Option<Arc<RenderImage>>,
    /// Why the file produced no raster.
    pub refusal: Option<RasterRefusal>,
}

/// One raster request; a job a newer request replaced is skipped before the expensive calls.
pub struct Job {
    pub path: PathBuf,
    pub size: (u32, u32),
    pub epoch: u64,
    pub latest: Arc<AtomicU64>,
    pub raster: Rasterizer,
}

/// A job's outcome: a raster, a refusal the card can name, or work a newer request replaced.
pub enum RasterOutcome {
    Raster(Arc<RenderImage>),
    Refused(RasterRefusal),
    Superseded,
}

/// The app renderer as a rasterizer; the caller owns neither the parse nor the renderer.
pub fn default_raster(renderer: SvgRenderer) -> Rasterizer {
    Arc::new(move |bytes: &[u8], (w, h): (u32, u32)| {
        let parsed = renderer
            .parse_svg_untrusted(bytes)
            .map_err(|e| e.to_string())?;
        let at = size(DevicePixels(w as i32), DevicePixels(h as i32));
        renderer
            .render_parsed(&parsed, SvgSize::ExactSize(at))
            .map_err(|e| e.to_string())
    })
}

/// The decoder a host-decoded file takes by its name: SVG renderer, libheif for HEIF containers, image crate otherwise.
pub fn for_path(path: &std::path::Path, renderer: SvgRenderer) -> Rasterizer {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    if is_vector_name(name) {
        default_raster(renderer)
    } else if is_heif_name(name) {
        super::decode::heif_raster(super::decode::LIBHEIF)
    } else {
        super::decode::picture_raster()
    }
}

/// Stat judges the path before any open, and the handle's own fstat catches a swap between the two.
fn read_bounded(path: &Path) -> Result<Vec<u8>, RasterRefusal> {
    // A row's exact text decodes to its own bytes, see docs/architecture/exact-names.md.
    let path = path
        .to_str()
        .map_or(std::borrow::Cow::Borrowed(path), flea::bytename::text_path);
    let path = &*path;
    if !std::fs::metadata(path).is_ok_and(|m| m.file_type().is_file()) {
        return Err(RasterRefusal::Unreadable);
    }
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(flea::oflags::O_NONBLOCK)
        .open(path)
        .map_err(|_| RasterRefusal::Unreadable)?;
    let meta = file.metadata().map_err(|_| RasterRefusal::Unreadable)?;
    // The kind is typed by name; only a regular file can be a preview the user asked to look at.
    if !meta.file_type().is_file() {
        return Err(RasterRefusal::Unreadable);
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    if !within_decode_bound(name, meta.len()) {
        return Err(RasterRefusal::TooLarge);
    }
    if is_compressed_vector(name) {
        return read_inflated(file);
    }
    let mut bytes = Vec::new();
    file.take(decode_byte_bound(name) + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| RasterRefusal::Unreadable)?;
    // A file that grew between the open and the read is over the bound when the read fills it.
    if !within_decode_bound(name, bytes.len() as u64) {
        return Err(RasterRefusal::TooLarge);
    }
    Ok(bytes)
}

/// Inflates a compressed vector, refusing past the same byte bound the plain read uses.
fn read_inflated(file: std::fs::File) -> Result<Vec<u8>, RasterRefusal> {
    let mut inflated = Vec::new();
    flate2::read::GzDecoder::new(file)
        .take(MAX_SVG_BYTES + 1)
        .read_to_end(&mut inflated)
        .map_err(|_| RasterRefusal::Unreadable)?;
    if !svg_within_byte_bound(inflated.len() as u64) {
        return Err(RasterRefusal::TooLarge);
    }
    Ok(inflated)
}

/// A job's whole work: the superseded check, a bounded read, the check again, then the rasterizer.
pub fn run(job: &Job) -> RasterOutcome {
    if job.latest.load(Ordering::SeqCst) != job.epoch {
        return RasterOutcome::Superseded;
    }
    // Host-decoded formats keep their byte bounds; other photos use scaled decoding.
    let own = job
        .path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(flea::ui_logic_pv1::pixels::is_host_decoded);
    if !own {
        return match super::photo::decode(&job.path, job.size, &|| {
            job.latest.load(Ordering::SeqCst) == job.epoch
        }) {
            Ok(image) if job.latest.load(Ordering::SeqCst) == job.epoch => {
                RasterOutcome::Raster(image)
            }
            Ok(_) => RasterOutcome::Superseded,
            Err(_) => RasterOutcome::Refused(RasterRefusal::Unreadable),
        };
    }
    let bytes = match read_bounded(&job.path) {
        Ok(bytes) => bytes,
        Err(refusal) => return RasterOutcome::Refused(refusal),
    };
    if job.latest.load(Ordering::SeqCst) != job.epoch {
        return RasterOutcome::Superseded;
    }
    match (job.raster)(&bytes, job.size) {
        Ok(image) => RasterOutcome::Raster(image),
        Err(reason) => {
            // One line per failed picture; the card shows the could-not-be-read sentence.
            eprintln!(
                "flea-gui: Quick Look could not draw {}: {reason}",
                job.path.display()
            );
            RasterOutcome::Refused(RasterRefusal::Unreadable)
        }
    }
}
