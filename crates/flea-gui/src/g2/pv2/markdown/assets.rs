//! What a document's layout reads from its host: picture sizes, figure states, formula boxes and fence colours; and the thread that decodes pictures.
use flea::ui_logic_pv2::markdown::{
    markdown_flow::FormulaBox,
    markdown_layout::{Assets, FigureState},
};
use gpui::{Image, ImageFormat, RenderImage, SvgRenderer};
use std::{
    collections::HashMap,
    io::Read,
    path::{Path, PathBuf},
    sync::{mpsc, Arc},
    thread,
};

/// A fence's answered lines, each line its runs of colour role and text.
pub type CodeLines = Arc<Vec<Vec<(i64, String)>>>;

/// The assets as one layout job reads them; the view swaps in a fresh one when any lands.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub pictures: HashMap<String, (f32, f32)>,
    /// A figure not here is still asked.
    pub figures: HashMap<usize, FigureState>,
    pub formulas: HashMap<usize, FormulaBox>,
    /// A fence's answered lines by its ordinal, each line's runs; a line not yet answered draws plain.
    pub code: HashMap<usize, CodeLines>,
}

impl Assets for Snapshot {
    fn picture(&self, url: &str) -> Option<(f32, f32)> {
        self.pictures.get(url).copied()
    }

    fn figure(&self, ordinal: usize) -> FigureState {
        self.figures
            .get(&ordinal)
            .copied()
            .unwrap_or(FigureState::Waiting)
    }

    fn formula(&self, ordinal: usize) -> Option<FormulaBox> {
        self.formulas.get(&ordinal).copied()
    }

    fn code(&self, fence: usize, line: usize) -> Option<Vec<(i64, String)>> {
        self.code.get(&fence)?.get(line.checked_sub(1)?).cloned()
    }
}

/// A picture file bigger than this is not read.
pub const PICTURE_MAX_BYTES: u64 = 32 * 1_048_576;

/// What the decoder draws: a picture file by its address, or a figure's or formula's SVG by its ordinal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Art {
    Figure(usize),
    Formula(usize),
}

enum Job {
    Picture {
        seq: u64,
        url: String,
        path: PathBuf,
        scale: f32,
        limit: u64,
    },
    Svg {
        seq: u64,
        art: Art,
        svg: Arc<str>,
        scale: f32,
    },
}

pub enum Decoded {
    /// The image and its natural size in logical pixels, None when the file is not a picture this decodes.
    Picture {
        seq: u64,
        url: String,
        image: Option<(Arc<RenderImage>, (f32, f32))>,
    },
    Svg {
        seq: u64,
        art: Art,
        image: Option<(Arc<RenderImage>, (f32, f32))>,
    },
}

/// The format a picture's first bytes name.
pub fn sniff(bytes: &[u8]) -> Option<ImageFormat> {
    let starts = |magic: &[u8]| bytes.starts_with(magic);
    if starts(b"\x89PNG") {
        Some(ImageFormat::Png)
    } else if starts(b"\xff\xd8") {
        Some(ImageFormat::Jpeg)
    } else if starts(b"GIF8") {
        Some(ImageFormat::Gif)
    } else if starts(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Some(ImageFormat::Webp)
    } else if starts(b"BM") {
        Some(ImageFormat::Bmp)
    } else if starts(b"II*\0") || starts(b"MM\0*") {
        Some(ImageFormat::Tiff)
    } else if starts(b"\0\0\x01\0") {
        Some(ImageFormat::Ico)
    } else {
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(1024)]).to_ascii_lowercase();
        head.contains("<svg").then_some(ImageFormat::Svg)
    }
}

fn draw(bytes: &[u8], scale: f32, svg: &SvgRenderer) -> Option<(Arc<RenderImage>, (f32, f32))> {
    let format = sniff(bytes)?;
    let (image, scale) = if format == ImageFormat::Svg {
        // GPUI rasterises an SVG at SMOOTH_SVG_SCALE_FACTOR times the asked scale.
        let pixels = scale * gpui::SMOOTH_SVG_SCALE_FACTOR;
        (svg.render_single_frame(bytes, scale).ok()?, pixels)
    } else {
        (
            Image::from_bytes(format, bytes.to_vec())
                .to_image_data(svg.clone())
                .ok()?,
            1.,
        )
    };
    let size = image.size(0);
    let natural = (
        (i32::from(size.width) as f32 / scale).round(),
        (i32::from(size.height) as f32 / scale).round(),
    );
    Some((image, natural))
}

fn picture(
    path: &Path,
    scale: f32,
    svg: &SvgRenderer,
    limit: u64,
) -> Option<(Arc<RenderImage>, (f32, f32))> {
    let flags = if limit < PICTURE_MAX_BYTES {
        flea::oflags::O_NOFOLLOW
    } else {
        0
    };
    let file = flea::backend::regfile::open_if_regular(path, flags).ok()?;
    if file.metadata().ok()?.len() > limit {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > limit {
        return None;
    }
    draw(&bytes, scale, svg)
}

/// The decoder thread's handle; its results land on `decoded`.
pub struct Decoder {
    jobs: mpsc::Sender<Job>,
    pub decoded: async_channel::Receiver<Decoded>,
    #[cfg(test)]
    _inert: Option<async_channel::Sender<Decoded>>,
}

impl Decoder {
    pub fn spawn(svg: SvgRenderer) -> Self {
        let (jobs, inbox) = mpsc::channel::<Job>();
        let (outbox, decoded) = async_channel::unbounded();
        #[cfg(test)]
        if crate::g2::pv2::render::MARKDOWN_INERT.get() {
            return Decoder {
                jobs,
                decoded,
                _inert: Some(outbox),
            };
        }
        let _ = thread::Builder::new()
            .name("flea-md-pictures".into())
            .spawn(move || {
                while let Ok(job) = inbox.recv() {
                    let reply = match job {
                        Job::Picture {
                            seq,
                            url,
                            path,
                            scale,
                            limit,
                        } => Decoded::Picture {
                            seq,
                            image: picture(&path, scale, &svg, limit),
                            url,
                        },
                        Job::Svg {
                            seq,
                            art,
                            svg: text,
                            scale,
                        } => Decoded::Svg {
                            seq,
                            art,
                            image: draw(text.as_bytes(), scale, &svg),
                        },
                    };
                    if outbox.send_blocking(reply).is_err() {
                        return;
                    }
                }
            });
        Decoder {
            jobs,
            decoded,
            #[cfg(test)]
            _inert: None,
        }
    }

    pub fn picture(&self, seq: u64, url: String, path: PathBuf, scale: f32) {
        self.picture_limited(seq, url, path, scale, PICTURE_MAX_BYTES);
    }

    pub fn picture_limited(&self, seq: u64, url: String, path: PathBuf, scale: f32, limit: u64) {
        let _ = self.jobs.send(Job::Picture {
            seq,
            url,
            path,
            scale,
            limit,
        });
    }

    pub fn svg(&self, seq: u64, art: Art, svg: Arc<str>, scale: f32) {
        let _ = self.jobs.send(Job::Svg {
            seq,
            art,
            svg,
            scale,
        });
    }
}

#[cfg(test)]
#[path = "assets_tests.rs"]
mod tests;
