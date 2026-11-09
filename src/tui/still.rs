use super::chafa::{self, Color, ColorMode, Grid};
use super::theme::{Depth, Theme};
use std::fs::File;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, RecvTimeoutError, TryRecvError},
    Arc,
};
use std::time::Duration;

// How often a waiting worker looks at its cancel flag.
const POLL: Duration = Duration::from_millis(20);

// Where a half-block picture comes from: the held file itself, a video's first frame, or a decoder's PNG.
pub enum Source {
    File(PathBuf),
    Poster(PathBuf, usize),
    Bytes(Vec<u8>),
}

// One picture as asked: which file and preview, which page view, the box in cells and the colours.
#[derive(Clone, Debug, PartialEq)]
pub struct Key {
    pub path: PathBuf,
    pub generation: usize,
    pub page: (usize, usize, usize),
    pub cells: (usize, usize),
    pub colors: ColorMode,
}

type Work = (Arc<AtomicBool>, Receiver<io::Result<Option<Grid>>>);

// The catalog key a picture that cannot be shown reports, worded when the status row says it.
pub const UNSHOWN: &str = "preview.unshown";
// The half-block picture for terminals without graphics: chafa in its sandbox, never on the input loop.
#[derive(Default)]
pub struct Still {
    key: Option<Key>,
    stage: Option<super::job::Job>,
    work: Option<Work>,
    retiring: Vec<Receiver<io::Result<Option<Grid>>>>,
    retiring_stages: Vec<Receiver<Result<Vec<u8>, String>>>,
    grid: Option<Grid>,
    // The last grid for this file, held across a page-key swap until the new key's grid lands.
    fallback: Option<Grid>,
    pub error: String,
    pub failure: Option<super::preview::words::Text>,
    places: Places,
}

// Where chafa is looked for and where a decoder's PNG is spooled; none means PATH and the private runtime directory.
#[derive(Clone, Default)]
struct Places {
    search: Option<std::ffi::OsString>,
    spool: Option<PathBuf>,
}

impl Still {
    // Asks for one picture; the picture already held or running for this key is kept.
    pub fn want(&mut self, key: Key, source: impl FnOnce() -> Option<Source>) {
        if self.key.as_ref() == Some(&key) {
            return;
        }
        // Only a page revision may reuse the last drawn grid while its replacement decodes.
        let keep = self.key.as_ref().is_some_and(|old| {
            old.path == key.path && old.generation == key.generation && old.cells == key.cells && old.colors == key.colors
        });
        let held = if keep { self.grid.take().or_else(|| self.fallback.take()) } else { None };
        self.clear();
        let Some(source) = source() else { return };
        self.fallback = held;
        self.key = Some(key);
        match source {
            Source::Poster(path, side) => {
                self.stage = Some(super::job::Job::start(path, super::graphics::poster_arguments(side)));
            }
            source => self.start(source),
        }
    }
    fn start(&mut self, source: Source) {
        let Some(key) = &self.key else { return };
        let cancel = Arc::new(AtomicBool::new(false));
        let stopped = cancel.clone();
        let (cells, colors, places) = (key.cells, key.colors, self.places.clone());
        let (tx, result) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(decode(source, cells, colors, places, &stopped));
        });
        self.work = Some((cancel, result));
    }
    // True when a new grid arrived; a missing chafa leaves the facts alone, with nothing said.
    pub fn poll(&mut self) -> bool {
        self.retiring.retain(|r| matches!(r.try_recv(), Err(TryRecvError::Empty)));
        self.retiring_stages.retain(|r| matches!(r.try_recv(), Err(TryRecvError::Empty)));
        if let Some(stage) = &self.stage {
            match stage.result.try_recv() {
                Ok(Ok(bytes)) => {
                    self.stage = None;
                    self.start(Source::Bytes(bytes));
                }
                Ok(Err(error)) => {
                    self.failure = Some(stage.failure(error.clone()));
                    self.stage = None;
                    self.fallback = None;
                    self.error = error;
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    self.stage = None;
                    self.fallback = None;
                    self.error = UNSHOWN.into();
                    self.failure = Some(super::preview::words::Text::Key(UNSHOWN));
                }
            }
        }
        let Some((_, result)) = &self.work else { return false };
        let outcome = match result.try_recv() {
            Err(TryRecvError::Empty) => return false,
            Err(TryRecvError::Disconnected) => Ok(None),
            Ok(outcome) => outcome,
        };
        self.work = None;
        match outcome {
            Ok(grid) => {
                let arrived = grid.is_some();
                self.grid = grid;
                self.fallback = None;
                arrived
            }
            Err(error) => {
                self.fallback = None;
                self.failure = Some(super::preview::words::from_io(&error));
                self.error = crate::error::io_message(&error);
                false
            }
        }
    }
    // Drops the picture and any work for it; a running chafa is cancelled and reaped off this thread.
    pub fn clear(&mut self) {
        if let Some((cancel, result)) = self.work.take() {
            cancel.store(true, Ordering::Relaxed);
            self.retiring.push(result);
        }
        if let Some(job) = self.stage.take() { self.retiring_stages.push(super::graphics::cancel_job(job)); }
        self.grid = None;
        self.fallback = None;
        self.key = None;
        self.error.clear();
        self.failure = None;
        self.retiring.retain(|r| matches!(r.try_recv(), Err(TryRecvError::Empty)));
        self.retiring_stages.retain(|r| matches!(r.try_recv(), Err(TryRecvError::Empty)));
    }
    pub fn key(&self) -> Option<&Key> {
        self.key.as_ref()
    }
    // The grid for this key only, so a stale picture is never drawn for the next file or size.
    pub fn grid(&self, key: &Key) -> Option<&Grid> {
        if self.key.as_ref() != Some(key) {
            return None;
        }
        self.grid.as_ref().or(self.fallback.as_ref())
    }
    #[cfg(test)]
    pub fn hold(&mut self, key: Key, grid: Grid) {
        self.clear();
        (self.key, self.grid) = (Some(key), Some(grid));
    }
    #[cfg(test)]
    pub fn running(&self) -> bool {
        self.work.is_some() || self.stage.is_some()
    }
}

impl Drop for Still {
    fn drop(&mut self) {
        self.clear();
        for result in self.retiring.drain(..) { let _ = result.recv_timeout(Duration::from_secs(12)); }
        for result in self.retiring_stages.drain(..) { let _ = result.recv_timeout(Duration::from_secs(12)); }
    }
}

// The colours chafa may use at this depth; a sixteen-colour terminal draws no half-block picture.
pub fn colors(depth: Depth) -> Option<ColorMode> {
    match depth {
        Depth::Truecolor => Some(ColorMode::Truecolor),
        Depth::Indexed => Some(ColorMode::Indexed),
        Depth::Sixteen | Depth::Mono => None,
    }
}

fn decode(source: Source, cells: (usize, usize), colors: ColorMode, places: Places, cancel: &AtomicBool) -> io::Result<Option<Grid>> {
    // A spooled PNG stays linked until this decode ends, so chafa's sandbox can bind it.
    let (input, _spooled) = match source {
        Source::File(path) => (super::job::held(&path)?, None),
        Source::Bytes(bytes) => spool(&bytes, places.spool).map(|(file, spooled)| (file, Some(spooled)))?,
        Source::Poster(..) => return Err(super::preview::words::issue(UNSHOWN, "Preview could not be shown")),
    };
    let job = match places.search {
        Some(search) => chafa::Job::start_on(input, cells.0, cells.1, colors, &search)?,
        None => chafa::Job::start(input, cells.0, cells.1, colors)?,
    };
    let Some(mut job) = job else { return Ok(None) };
    loop {
        // This worker is off the input loop, so a cancel waits here until chafa is killed and reaped.
        if cancel.load(Ordering::Relaxed) {
            job.cancel();
            return Err(super::preview::words::issue("tui.preview.cancelled", "Preview cancelled"));
        }
        match job.result.recv_timeout(POLL) {
            Ok(grid) => return grid.map(Some),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Err(super::preview::words::issue(UNSHOWN, "Preview could not be shown")),
        }
    }
}

// A spooled PNG's name, unlinked when the decode that bound it ends.
pub(super) struct Spooled(pub PathBuf);
impl Drop for Spooled {
    fn drop(&mut self) {
        if self.0.is_absolute() {
            let _ = std::fs::remove_file(&self.0);
        }
    }
}

// A decoder's PNG as a new file in an owner-only directory: the sandbox binds a file by name, so a memfd or unlinked file cannot reach chafa.
pub(super) fn spool(bytes: &[u8], directory: Option<PathBuf>) -> io::Result<(File, Spooled)> {
    use crate::backend::manifestdir::{candidate_dirs, current_uid, qualifying_dirs};
    use std::os::unix::fs::OpenOptionsExt;
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let directory = directory.or_else(|| qualifying_dirs(&candidate_dirs(current_uid()), &[], current_uid()).into_iter().next())
        .ok_or_else(|| super::preview::words::issue(UNSHOWN, "Preview could not be shown"))?;
    let path = directory.join(format!("still-{}-{}.png", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    let mut file = std::fs::OpenOptions::new().read(true).write(true).create_new(true).mode(0o600).custom_flags(crate::oflags::O_NOFOLLOW).open(&path)?;
    let spooled = Spooled(path);
    file.write_all(bytes)?;
    Ok((file, spooled))
}

// Shared 3D buffers and their one transport input remain reusable across stepped views.
pub struct ModelStill {
    pub key: super::model3d::Key,
    pub image: Spooled,
    scene: std::sync::Arc<flea::ui_logic_pv4::scene::Scene>,
    raster: flea::ui_logic_pv4::raster::Raster,
    pub rgba: Vec<u8>,
}

pub fn render_model(held: std::sync::Arc<super::model3d::Held>, file: PathBuf, key: super::model3d::Key, old: Option<ModelStill>, directory: Option<PathBuf>, cancel: &AtomicBool) -> Result<ModelStill, String> {
    use flea::ui_logic_pv4::{camera, control::{Surface, Viewer}, motion, raster::{Prepared, Style}, scene::{self, CPU_LIVE_TRIANGLES}};
    if cancel.load(Ordering::Relaxed) || !held.fresh() { return Err("model source changed".into()); }
    let scene = match &old {
        Some(old) if old.key.path == key.path && old.key.generation == key.generation => old.scene.clone(),
        _ => std::sync::Arc::new(super::model3d::scene(&file, cancel)?),
    };
    if cancel.load(Ordering::Relaxed) { return Err("model render cancelled".into()); }
    let mut live = scene::decimate(&scene, key.controls.plate, CPU_LIVE_TRIANGLES);
    live.keep_extrusions();
    let home = camera::Home::new(&live, 0).ok_or("empty model plate")?;
    let (w, h) = key.pixels;
    let mut viewer = Viewer::from_home(Surface::Terminal, home, (key.controls.plate, scene.plate_count()), live.layers.len(), w as f32, h as f32);
    viewer.view.az -= key.controls.orbit as f32 * motion::ORBIT_KEY;
    viewer.view.zoom_at(viewer.view.cx, viewer.view.cy, motion::ZOOM_KEY.powi(key.controls.zoom), camera::zoom_limits(home.radius, h as f32));
    let prepared = Prepared::new(&live, key.palette);
    let edges = if key.controls.wire && !live.is_toolpath() { Some(scene::edges(&live, 0)) } else { None };
    let style = Style { palette: key.palette, wire: edges.as_deref(), layer: viewer.layer, plate: Some(home.rect), ground: Some(key.palette.ground) };
    let (mut raster, mut rgba) = old.map(|old| (old.raster, old.rgba)).unwrap_or_default();
    if cancel.load(Ordering::Relaxed) { return Err("model render cancelled".into()); }
    raster.resize(w, h, 1);
    raster.render(&live, &prepared, 0, &viewer.view, &style);
    raster.rgba_into(&mut rgba);
    if cancel.load(Ordering::Relaxed) || !held.fresh() { return Err("model source changed".into()); }
    let mut ppm = format!("P6\n{w} {h}\n255\n").into_bytes();
    ppm.reserve(w * h * 3);
    for pixel in rgba.chunks_exact(4) { ppm.extend_from_slice(&pixel[..3]); }
    let (_, image) = spool(&ppm, directory).map_err(|e| e.to_string())?;
    Ok(ModelStill { key, image, scene, raster, rgba })
}

// The QOI specification's framing: magic, then width and height, three channels in sRGB, one RGB op per pixel, the end marker.
const QOI_MAGIC: &[u8; 4] = b"qoif";
const QOI_CHANNELS_SRGB: [u8; 2] = [3, 0];
const QOI_OP_RGB: u8 = 0xfe;
const QOI_END: [u8; 8] = [0, 0, 0, 0, 0, 0, 0, 1];
const QOI_HEADER_BYTES: usize = 14;
// RGBA pixels as an uncompressed QOI, every pixel one QOI_OP_RGB, a format chafa decodes where it refuses PPM.
pub(super) fn qoi(rgba: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(QOI_HEADER_BYTES + width * height * 4 + QOI_END.len());
    out.extend_from_slice(QOI_MAGIC);
    out.extend_from_slice(&(width as u32).to_be_bytes());
    out.extend_from_slice(&(height as u32).to_be_bytes());
    out.extend_from_slice(&QOI_CHANNELS_SRGB);
    for pixel in rgba.chunks_exact(4).take(width * height) {
        out.extend_from_slice(&[QOI_OP_RGB, pixel[0], pixel[1], pixel[2]]);
    }
    out.extend_from_slice(&QOI_END);
    out
}

// One picture row in cells: only chafa's validated symbols and colours, padded with the ground to width.
pub fn row(grid: &Grid, y: usize, width: usize, theme: &Theme) -> String {
    let mut out = theme.base();
    let mut used = 0;
    let mut last = None;
    let start = (y * grid.columns).min(grid.cells.len());
    let cells = if y < grid.rows { &grid.cells[start..(start + grid.columns).min(grid.cells.len())] } else { &[] };
    for cell in cells.iter().take(width) {
        if last != Some((cell.foreground, cell.background)) {
            out.push_str(&ink(cell.foreground, false, theme));
            out.push_str(&ink(cell.background, true, theme));
            last = Some((cell.foreground, cell.background));
        }
        out.push(cell.symbol);
        used += 1;
    }
    out.push_str(&theme.base());
    out.push_str(&" ".repeat(width - used));
    out
}

fn ink(color: Color, background: bool, theme: &Theme) -> String {
    let layer = if background { 48 } else { 38 };
    match color {
        Color::Default if background => theme.background.clone(),
        Color::Default => theme.foreground.clone(),
        Color::Rgb(r, g, b) => format!("\x1b[{layer};2;{r};{g};{b}m"),
        Color::Indexed(index) => format!("\x1b[{layer};5;{index}m"),
    }
}

#[cfg(test)]
#[path = "still_tests.rs"]
mod tests;
