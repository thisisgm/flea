use super::job::Job;
use std::path::PathBuf;

// A bounded preview worker stays owned until completion, including cancellation and quit.
pub struct Task<T> {
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    result: std::sync::mpsc::Receiver<Result<T, String>>,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl<T: Send + 'static> Task<T> {
    pub fn start(work: impl FnOnce(&std::sync::atomic::AtomicBool) -> Result<T, String> + Send + 'static) -> Self {
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stopped = cancel.clone();
        let (tx, result) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || { let _ = tx.send(work(&stopped)); });
        Self { cancel, result, worker: Some(worker) }
    }
    pub fn cancel(&self) { self.cancel.store(true, std::sync::atomic::Ordering::Relaxed); }
    pub fn cancelled(&self) -> bool { self.cancel.load(std::sync::atomic::Ordering::Relaxed) }
    pub fn poll(&mut self) -> Option<Result<T, String>> {
        if !self.worker.as_ref().is_some_and(|worker| worker.is_finished()) { return None; }
        let _ = self.worker.take().unwrap().join();
        Some(self.result.try_recv().unwrap_or_else(|_| Err("model worker stopped".into())))
    }
    pub fn discard_cancelled(&mut self) -> bool {
        self.cancel.load(std::sync::atomic::Ordering::Relaxed) && self.poll().is_some()
    }
}
impl<T> Drop for Task<T> {
    fn drop(&mut self) {
        self.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(worker) = self.worker.take() { let _ = worker.join(); }
    }
}

// Read the terminal's encoded theme roles back into the shared renderer's sRGB palette.
pub fn palette(theme: &super::theme::Theme) -> flea::ui_logic_model_preview::color::Palette {
    if theme.depth == super::theme::Depth::Mono {
        // Raster pixels cannot inherit terminal attributes or its default ink.
        return flea::ui_logic_model_preview::color::Palette {
            ground: [0.; 3],
            foreground: [1.; 3],
            muted: [0.7; 3],
            frame: [0.5; 3],
        };
    }
    let ground = hex(&theme.ground);
    let role = |text: &str| {
        let values: Vec<u8> = text.trim_start_matches("\x1b[").trim_end_matches('m').split(';').filter_map(|s| s.parse().ok()).collect();
        // Sixteen-colour roles arrive as "\x1b[37m" or "\x1b[97m" and read through the same basic table.
        let values = match values.as_slice() {
            [n @ 30..=37] => vec![38, 5, n - 30],
            [n @ 90..=97] => vec![38, 5, n - 90 + 8],
            _ => values,
        };
        match values.as_slice() {
            [38, 2, r, g, b] => [*r, *g, *b].map(|v| v as f32 / 255.),
            [38, 5, n] if *n >= 232 => [((f32::from(*n) - 232.) * 10. + 8.) / 255.; 3],
            [38, 5, n] if *n >= 16 => {
                let n = n - 16;
                [n / 36, n / 6 % 6, n % 6].map(|v| if v == 0 { 0. } else { (55. + 40. * f32::from(v)) / 255. })
            }
            [38, 5, n] => {
                const BASIC: [[u8; 3]; 16] = [[0,0,0],[128,0,0],[0,128,0],[128,128,0],[0,0,128],[128,0,128],[0,128,128],[192,192,192],
                    [128,128,128],[255,0,0],[0,255,0],[255,255,0],[0,0,255],[255,0,255],[0,255,255],[255,255,255]];
                BASIC[*n as usize].map(|v| v as f32 / 255.)
            }
            _ => ground,
        }
    };
    flea::ui_logic_model_preview::color::Palette { ground, foreground: role(&theme.foreground), muted: role(&theme.meta), frame: role(&theme.track) }
}
fn hex(text: &str) -> [f32; 3] {
    let text = text.strip_prefix('#').unwrap_or("");
    std::array::from_fn(|i| text.get(i * 2..i * 2 + 2).and_then(|s| u8::from_str_radix(s, 16).ok()).map_or(0., |v| v as f32 / 255.))
}

pub use super::capabilities::Protocol;
pub struct Graphics {
    pub protocol: Protocol,
    job: Option<Job>,
    retiring: Vec<std::sync::mpsc::Receiver<Result<Vec<u8>, String>>>,
    identity: PathBuf,
    pub bytes: Vec<u8>,
    // Kitty's one transmission of the image; each changed frame after it only re-places the stored copy.
    pub transmit: Vec<u8>,
    pub error: String,
    pub failure: Option<super::preview::words::Text>,
    pub columns: usize,
    pub rows: usize,
    pixels: (usize, usize),
}
impl Graphics {
    pub fn new() -> Self {
        Self {
            protocol: Protocol::None,
            job: None,
            retiring: Vec::new(),
            identity: PathBuf::new(),
            bytes: Vec::new(),
            transmit: Vec::new(),
            error: String::new(),
            failure: None,
            columns: 0,
            rows: 0,
            pixels: (0, 0),
        }
    }
    pub fn request(&mut self, path: PathBuf, columns: usize, rows: usize, pixels: (usize, usize)) {
        if !self.begin(&path, columns, rows, pixels) {
            return;
        }
        let format = if self.protocol == Protocol::Kitty {
            "png:-"
        } else {
            "sixel:-"
        };
        self.job = Some(Job::start(
            path,
            vec![
                "/usr/bin/magick".into(),
                "{input}".into(),
                "-resize".into(),
                format!("{}x{}", pixels.0, pixels.1),
                format.into(),
            ],
        ));
    }
    // A video's first frame at its fitted size, what a paused mpv draws, from one ffmpegthumbnailer run (kitty only).
    pub fn request_poster(&mut self, path: PathBuf, columns: usize, rows: usize, pixels: (usize, usize), side: usize) {
        if self.protocol != Protocol::Kitty || !self.begin(&path, columns, rows, pixels) {
            return;
        }
        self.job = Some(Job::start(path, poster_arguments(side)));
    }
    pub fn holds(&self, path: &std::path::Path) -> bool {
        self.identity == path
    }
    pub fn shows(&self, path: &std::path::Path) -> bool {
        self.identity == path && !self.bytes.is_empty()
    }
    // Forgets the image without deleting it: mpv's forwarder deletes the poster once its own frame is up.
    pub fn release(&mut self) {
        self.retire();
        self.bytes.clear();
        self.transmit.clear();
        self.identity = PathBuf::new();
    }
    // False when this image is already asked for at this size; otherwise the old one is cleared for it.
    fn begin(&mut self, path: &std::path::Path, columns: usize, rows: usize, pixels: (usize, usize)) -> bool {
        if self.protocol == Protocol::None
            || (self.identity == path && self.columns == columns && self.rows == rows && self.pixels == pixels)
        {
            return false;
        }
        self.clear();
        self.identity = path.into();
        self.columns = columns;
        self.rows = rows;
        self.pixels = pixels;
        true
    }
    pub fn accept(&mut self) -> bool {
        self.retiring.retain(|r| matches!(r.try_recv(), Err(std::sync::mpsc::TryRecvError::Empty)));
        let Some(job) = &self.job else {
            return false;
        };
        let Ok(result) = job.result.try_recv() else {
            return false;
        };
        self.failure = result.as_ref().err().map(|error| job.failure(error.clone()));
        self.job = None;
        match result {
            Ok(bytes) => {
                if self.protocol == Protocol::Kitty {
                    self.transmit = kitty_chunks(&bytes, 't');
                    self.bytes = PLACE.to_vec();
                } else {
                    self.bytes = bytes;
                }
            }
            Err(error) => self.error = error,
        }
        true
    }
    pub fn clear(&mut self) {
        if self.protocol == Protocol::Kitty && (!self.bytes.is_empty() || !self.identity.as_os_str().is_empty()) {
            let _ = super::tmux::write_graphics(&mut std::io::stdout().lock(), b"\x1b_Ga=d,d=I,i=42,q=2\x1b\\");
        }
        self.retire();
        self.bytes.clear();
        self.transmit.clear();
        self.error.clear();
        self.failure = None;
        self.identity = PathBuf::new();
    }
    fn retire(&mut self) {
        if let Some(job) = self.job.take() { self.retiring.push(cancel_job(job)); }
        self.retiring.retain(|r| matches!(r.try_recv(), Err(std::sync::mpsc::TryRecvError::Empty)));
    }
}
impl Drop for Graphics {
    fn drop(&mut self) {
        self.clear();
        for result in self.retiring.drain(..) { let _ = result.recv_timeout(std::time::Duration::from_secs(12)); }
    }
}

// Keep the completion channel after Drop signals cancellation, so quit can wait for the worker's reap.
pub(super) fn cancel_job(mut job: Job) -> std::sync::mpsc::Receiver<Result<Vec<u8>, String>> {
    let (_, empty) = std::sync::mpsc::channel();
    std::mem::replace(&mut job.result, empty)
}
const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
pub(crate) fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let a = chunk[0] as usize;
        let b = chunk.get(1).copied().unwrap_or(0) as usize;
        let c = chunk.get(2).copied().unwrap_or(0) as usize;
        out.push(BASE64[a >> 2] as char);
        out.push(BASE64[((a & 3) << 4) | (b >> 4)] as char);
        out.push(if chunk.len() > 1 {
            BASE64[((b & 15) << 2) | (c >> 6)] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            BASE64[c & 63] as char
        } else {
            '='
        });
    }
    out
}
// The inverse of base64 for one kitty payload; None for a byte outside the alphabet or a length that is not whole.
pub(crate) fn unbase64(text: &[u8]) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(4) {
        return None;
    }
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    for quad in text.chunks(4) {
        let pad = quad.iter().rev().take_while(|&&b| b == b'=').count();
        if pad > 2 {
            return None;
        }
        let mut bits = 0u32;
        for &b in &quad[..4 - pad] {
            bits = bits << 6 | BASE64.iter().position(|&c| c == b)? as u32;
        }
        bits <<= 6 * pad as u32;
        out.extend_from_slice(&[(bits >> 16) as u8, (bits >> 8) as u8, bits as u8][..3 - pad]);
    }
    Some(out)
}
pub const POSTER: &str = "/usr/bin/ffmpegthumbnailer";
// One ffmpegthumbnailer run for a video's first frame as PNG, its longer side this many pixels.
pub fn poster_arguments(side: usize) -> Vec<String> {
    [POSTER, "-i", "{input}", "-o", "-", "-c", "png", "-s", &side.to_string(), "-t", "0"].map(String::from).to_vec()
}
// The cell size in pixels when the terminal reports none, so half-block pictures still have a box.
pub const FALLBACK_CELL: (usize, usize) = (8, 16);
// The longest side of a w x h picture fitted inside the pane's pixels; an unknown size fits either orientation.
pub fn poster_side(size: (usize, usize), pixels: (usize, usize)) -> usize {
    let (w, h) = size;
    if w == 0 || h == 0 {
        return pixels.0.min(pixels.1);
    }
    let scale = (pixels.0 as f64 / w as f64).min(pixels.1 as f64 / h as f64);
    (w.max(h) as f64 * scale).floor().max(1.0) as usize
}
pub const PLACE: &[u8] = b"\x1b_Ga=p,i=42,q=2\x1b\\";
// A picture's transmission is the TUI's largest allocation, so the heap pages it freed go back once it is written.
pub fn send(out: &mut impl std::io::Write, transmit: Vec<u8>) -> std::io::Result<()> {
    if transmit.is_empty() {
        return Ok(());
    }
    super::tmux::write_graphics(out, &transmit)?;
    drop(transmit);
    crate::heap::give_back();
    Ok(())
}
// Drawn at the picture's own pixel size, already fitted to the pane: kitty stretches an image given both c and r.
pub fn kitty(bytes: &[u8]) -> Vec<u8> {
    kitty_chunks(bytes, 'T')
}
// The image as 4 KiB base64 chunks under id 42: action T transmits and shows it, t only stores it for PLACE.
pub fn kitty_chunks(bytes: &[u8], action: char) -> Vec<u8> {
    const CHUNK: usize = 4096;
    let encoded = base64(bytes);
    let mut out = String::new();
    let chunks = encoded.as_bytes().chunks(CHUNK);
    let count = chunks.len();
    for (i, chunk) in chunks.enumerate() {
        let more = usize::from(i + 1 < count);
        let header = if i == 0 {
            format!("a={},f=100,i=42,q=2,m={}", action, more)
        } else {
            format!("m={}", more)
        };
        out.push_str(&format!(
            "\x1b_G{};{}\x1b\\",
            header,
            std::str::from_utf8(chunk).unwrap_or("")
        ));
    }
    out.into_bytes()
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tui_model3d_graphics_cancellation_reaps_decoder() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
        use std::time::{Duration, Instant};
        assert!(crate::tui::chafa::test_support::sandboxed());
        let root = crate::tui::model3d::tests::Fixture::new();
        let input = root.path("input");
        std::fs::write(&input, b"held\n").unwrap();
        let marker = root.path("owned-decoder").to_string_lossy().into_owned();
        let children = || std::fs::read_dir("/proc").unwrap().flatten().filter(|entry| {
            std::fs::read(entry.path().join("cmdline")).is_ok_and(|line| line.split(|b| *b == 0).any(|arg| arg == marker.as_bytes()))
        }).count();
        for protocol in [Protocol::Kitty, Protocol::Sixel] {
            let mut graphics = Graphics::new();
            graphics.protocol = protocol;
            graphics.job = Some(Job::start(input.clone(), vec!["/bin/sh".into(), "-c".into(), "trap '' TERM; while :; do /usr/bin/sleep 60; done".into(), marker.clone()]));
            let until = Instant::now() + Duration::from_secs(10);
            while children() == 0 { assert!(Instant::now() < until); std::thread::sleep(Duration::from_millis(5)); }
            graphics.clear();
            while !graphics.retiring.is_empty() {
                graphics.accept();
                assert!(Instant::now() < until, "decoder was not reaped after cancellation");
                std::thread::sleep(Duration::from_millis(5));
            }
            assert_eq!(children(), 0);
        }
    }

    #[test]
    fn unbase64_inverts_base64_and_refuses_what_it_did_not_make() {
        for bytes in [&b""[..], b"/", b"/m", b"/mp", b"/mpv-kitty-0x55d688f18d70"] {
            assert_eq!(unbase64(base64(bytes).as_bytes()).as_deref(), Some(bytes));
        }
        for text in [&b"abc"[..], b"ab!=", b"a===", b"===="] {
            assert_eq!(unbase64(text), None, "{}", String::from_utf8_lossy(text));
        }
    }
    #[test]
    fn kitty_encoding_is_bounded_and_exact() {
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
    }
    #[test]
    fn a_kitty_image_carries_no_cell_box_to_stretch_into() {
        let out = String::from_utf8(kitty(&[0u8; 5000])).unwrap();
        assert!(out.starts_with("\x1b_Ga=T,f=100,i=42,q=2,m=1;"), "{}", &out[..40]);
        assert!(!out.contains(",c=") && !out.contains(",r="));
        assert!(out.ends_with("\x1b\\") && out.contains("\x1b_Gm=0;"));
    }
    #[test]
    fn a_poster_fills_the_pane_on_its_limiting_side() {
        assert_eq!(poster_side((1920, 1080), (760, 820)), 760);
        assert_eq!(poster_side((1080, 1920), (760, 820)), 820);
        assert_eq!(poster_side((1920, 1080), (1000, 400)), 711);
        assert_eq!(poster_side((0, 0), (760, 820)), 760);
    }
    #[test]
    fn a_stored_kitty_image_is_sent_once_and_placed_by_id() {
        let out = String::from_utf8(kitty_chunks(&[0u8; 5000], 't')).unwrap();
        assert!(out.starts_with("\x1b_Ga=t,f=100,i=42,q=2,m=1;") && out.contains("\x1b_Gm=0;"));
        assert_eq!(PLACE, b"\x1b_Ga=p,i=42,q=2\x1b\\");
    }
    #[test]
    fn a_small_kitty_image_is_one_chunk() {
        assert!(String::from_utf8(kitty(b"png")).unwrap().starts_with("\x1b_Ga=T,f=100,i=42,q=2,m=0;"));
    }
}
