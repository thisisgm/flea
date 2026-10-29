// Existing desktop priors are a bounded read-only snapshot; no candidate is statted for ranking.
use super::searchindex::{Index, Scratch};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Once, OnceLock};
use std::time::{Duration, Instant};

#[path = "searchpriorxml.rs"]
mod xml;
use xml::xbel;

const BYTES: usize = 1 << 20;
const RECORDS: usize = 4096;
const PATH_BYTES: usize = 4096;
const LIMIT: Duration = Duration::from_secs(2);
const SCALE: i32 = 64;
// The backend's load waits this long first, so its file read and zoxide spawn stay clear of the first window.
const GRACE: Duration = Duration::from_secs(1);
static GLOBAL: OnceLock<Load> = OnceLock::new();
static STARTED: Once = Once::new();

// One snapshot load on its own worker; an open reads whatever it has finished and never waits for it.
#[derive(Default)]
pub(crate) struct Load {
    snapshot: OnceLock<Priors>,
}

#[derive(Default)]
pub(crate) struct Priors {
    files: BTreeMap<Vec<u8>, u8>,
    folders: BTreeMap<Vec<u8>, (u8, u8)>,
    // The folder of every kept file, so a folder holding none never needs a file's own path.
    file_folders: BTreeSet<Vec<u8>>,
    bytes: usize,
}

// A folder's slot in apply: 0 until seen, then 1 plus the context and proximity its files share, BESIDE when it holds a prior file.
const BESIDE: u8 = 0x80;

impl Priors {
    fn new(files: Vec<Vec<u8>>, folders: Vec<Vec<u8>>) -> Self {
        let mut out = Self::default();
        for (paths, directory) in [(&folders, true), (&files, false)] {
            for (at, path) in paths.iter().enumerate() {
                let strength = 8 - (at * 8 / paths.len()).min(7) as u8;
                out.insert(path, directory, strength, strength);
                if !directory {
                    if let Some(parent) = parent(path) { out.insert(parent, true, 0, strength); }
                }
            }
        }
        out.file_folders = out.files.keys().filter_map(|path| parent(path)).map(<[u8]>::to_vec).collect();
        out
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.files.is_empty() && self.folders.is_empty()
    }

    fn insert(&mut self, path: &[u8], directory: bool, direct: u8, context: u8) {
        let exists = if directory { self.folders.contains_key(path) } else { self.files.contains_key(path) };
        if !exists && (self.files.len() + self.folders.len() >= RECORDS || self.bytes + path.len() > BYTES) { return; }
        if !exists { self.bytes += path.len(); }
        if directory {
            let kept = self.folders.entry(path.to_vec()).or_default();
            kept.0 = kept.0.max(direct);
            kept.1 = kept.1.max(context);
        } else {
            let kept = self.files.entry(path.to_vec()).or_default();
            *kept = (*kept).max(direct);
        }
    }

    fn apply(&self, scope: &Path, folder: &Path, index: &Index, hits: &mut [(i32, usize)]) {
        let base = scope.as_os_str().as_bytes();
        let inside = |path: &Vec<u8>| path.strip_prefix(base).is_some_and(|rest| rest.is_empty() || base.ends_with(b"/") || rest.starts_with(b"/"));
        if !self.files.keys().chain(self.folders.keys()).any(inside) { return; }
        let mut path = scope.as_os_str().as_bytes().to_vec();
        if !path.ends_with(b"/") { path.push(b'/'); }
        let prefix = path.len();
        let mut scratch = Scratch::default();
        let folder = folder.as_os_str().as_bytes();
        let mut slots = vec![0u8; index.folder_slots()];
        for (score, row) in hits {
            let file = !index.is_dir(*row);
            let slot = if file { slots[index.folder_slot(*row)] } else { 0 };
            let (direct, shared) = if slot != 0 && slot & BESIDE == 0 { (0, i32::from(slot) - 1) } else {
                path.truncate(prefix);
                path.extend_from_slice(index.raw_into(*row, &mut scratch));
                let direct = if file { self.files.get(&path).copied() } else { self.folders.get(&path).map(|v| v.0) }.unwrap_or(0);
                let shared = if slot != 0 { i32::from(slot & !BESIDE) - 1 } else {
                    self.context(if file { parent(&path) } else { Some(path.as_slice()) })
                        + parent(&path).map_or(0, |dir| 7 - distance(dir, folder).min(7) as i32)
                };
                if file && slot == 0 {
                    let beside = parent(&path).is_some_and(|dir| self.file_folders.contains(dir));
                    slots[index.folder_slot(*row)] = (shared + 1) as u8 | if beside { BESIDE } else { 0 };
                }
                (direct, shared)
            };
            let bonus = if direct > 0 { 24 + i32::from(direct) * 2 } else { 0 };
            // At most 63: even a one-point lexical advantage survives every prior and distance bonus.
            *score = *score * SCALE + bonus + shared;
        }
    }

    // Folder strengths up the ancestry from `start`, two points less a level, over at most eight levels.
    fn context(&self, mut ancestor: Option<&[u8]>) -> i32 {
        let mut context = 0;
        for distance in 0..8 {
            let Some(dir) = ancestor else { break };
            if let Some((_, strength)) = self.folders.get(dir) {
                context = context.max((i32::from(*strength) * 2 - distance * 2).max(0));
            }
            ancestor = parent(dir);
        }
        context
    }
}

fn parent(path: &[u8]) -> Option<&[u8]> {
    if path == b"/" { return None; }
    path.iter().rposition(|&b| b == b'/').map(|at| &path[..at.max(1)])
}

fn distance(a: &[u8], b: &[u8]) -> usize {
    let (mut a, mut b) = (a.split(|&c| c == b'/').filter(|p| !p.is_empty()), b.split(|&c| c == b'/').filter(|p| !p.is_empty()));
    loop {
        match (a.next(), b.next()) {
            (Some(a), Some(b)) if a == b => {}
            (a_next, b_next) => return a.count() + b.count() + usize::from(a_next.is_some()) + usize::from(b_next.is_some()),
        }
    }
}

impl Load {
    fn spawn(&'static self, source: impl FnOnce() -> Priors + Send + 'static) {
        let launched = std::thread::Builder::new().name("search-prior-load".into()).spawn(move || { let _ = self.snapshot.set(source()); });
        if launched.is_err() { let _ = self.snapshot.set(Priors::default()); }
    }

    // The snapshot if the load has finished, never waited for.
    pub(crate) fn loaded(&self) -> Option<&Priors> {
        self.snapshot.get()
    }

    // A loader over named sources, so a test never reads or changes the process environment.
    #[cfg(test)]
    pub(crate) fn leaked(history: Option<PathBuf>, program: PathBuf) -> &'static Load {
        let load: &'static Load = Box::leak(Box::default());
        load.spawn(move || load_from(history, program, LIMIT));
        load
    }
}

// The process's one load, started by the first list or search and done about GRACE later; an open before then ranks without it.
pub(crate) fn start() {
    let load = GLOBAL.get_or_init(Load::default);
    STARTED.call_once(|| load.spawn(|| { std::thread::sleep(GRACE); self::load() }));
}

// The process's snapshot if its load has finished; None before start or while it runs.
pub(crate) fn loaded() -> Option<&'static Priors> {
    GLOBAL.get().and_then(Load::loaded)
}

// Bonuses for part of a match set of `matched` hits, the same ones scaling the whole set gives them.
pub(crate) fn scale(priors: Option<&Priors>, scope: &Path, folder: &Path, index: &Index, hits: &mut [(i32, usize)], matched: usize) {
    if matched > 1 {
        if let Some(priors) = priors.filter(|priors| !priors.is_empty()) { priors.apply(scope, folder, index, hits); }
    }
}

fn history_path() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from).filter(|p| p.is_absolute()).map(|p| p.join(".local/share")))
        .map(|p| p.join("recently-used.xbel"))
}

fn load() -> Priors {
    load_from(history_path(), PathBuf::from("zoxide"), LIMIT)
}

fn load_from(path: Option<PathBuf>, program: PathBuf, limit: Duration) -> Priors {
    let deadline = Instant::now() + limit;
    let (tx, rx) = std::sync::mpsc::sync_channel(2);
    let files_tx = tx.clone();
    // Each source gets one worker: even opening a regular file or spawning an unavailable executable can block.
    let _ = std::thread::Builder::new().name("search-priors".into()).spawn(move || {
        let files = path.and_then(|p| read_xbel(&p)).unwrap_or_default();
        let _ = files_tx.send((false, files));
    });
    let _ = std::thread::Builder::new().name("search-zoxide".into()).spawn(move || {
        let _ = tx.send((true, zoxide(&program, deadline).unwrap_or_default()));
    });
    let (mut files, mut folders) = (Vec::new(), Vec::new());
    for _ in 0..2 {
        match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok((false, rows)) => files = rows,
            Ok((true, rows)) => folders = rows,
            Err(_) => break,
        }
    }
    Priors::new(files, folders)
}

fn read_xbel(path: &Path) -> Option<Vec<Vec<u8>>> {
    let (file, meta) = super::regfile::open_if_regular_with_meta(path, crate::oflags::O_NOFOLLOW).ok()?;
    if meta.len() > BYTES as u64 { return None; }
    let mut bytes = Vec::new();
    file.take(BYTES as u64 + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() > BYTES { return None; }
    xbel(std::str::from_utf8(&bytes).ok()?)
}

// This uses folder jump's read-only argv and 2 s/1 MiB/1000-row limits, without its existence checks or stale fallback.
fn zoxide(program: &Path, deadline: Instant) -> Option<Vec<Vec<u8>>> {
    extern "C" { fn fcntl(fd: i32, cmd: i32, ...) -> i32; }
    let mut child = Command::new(program).args(["query", "--list", "--all", "--score"])
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    let result = (|| {
        let mut pipe = child.stdout.take()?;
        // Linux F_GETFL/F_SETFL preserve existing flags while making even an inherited pipe deadline-safe.
        let flags = unsafe { fcntl(pipe.as_raw_fd(), 3, 0) };
        if flags < 0 || unsafe { fcntl(pipe.as_raw_fd(), 4, flags | crate::oflags::O_NONBLOCK) } < 0 { return None; }
        let mut text = Vec::new();
        let mut buffer = [0; 8192];
        let mut closed = false;
        loop {
            if Instant::now() >= deadline { return None; }
            match pipe.read(&mut buffer) {
                Ok(0) => closed = true,
                Ok(n) => { if text.len() + n > BYTES { return None; } text.extend_from_slice(&buffer[..n]); }
                Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted) => {}
                Err(_) => return None,
            }
            if closed {
                if let Some(status) = child.try_wait().ok()? { return if status.success() { zoxide_paths(&text) } else { None }; }
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    })();
    let _ = child.kill();
    // A killed child blocked in kernel I/O must not extend the provider deadline.
    let _ = std::thread::Builder::new().name("search-prior-reap".into()).spawn(move || { let _ = child.wait(); });
    result
}

fn zoxide_paths(text: &[u8]) -> Option<Vec<Vec<u8>>> {
    let mut rows = Vec::new();
    for line in text.split(|&b| b == b'\n').filter(|l| !l.is_empty()) {
        let start = line.iter().position(|c| !c.is_ascii_whitespace()).unwrap_or(line.len());
        let line = &line[start..];
        let at = line.iter().position(|&b| b == b' ')?;
        let score: f64 = std::str::from_utf8(&line[..at]).ok()?.parse().ok()?;
        let path = &line[at + 1..];
        if !score.is_finite() || score < 0.0 || !local_path(path) { return None; }
        if rows.len() < 1000 { rows.push((path.to_vec(), score)); }
    }
    rows.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    Some(rows.into_iter().map(|(path, _)| path).collect())
}

fn local_path(path: &[u8]) -> bool {
    path.starts_with(b"/") && path.len() > 1 && path.len() <= PATH_BYTES && !path.iter().any(|&b| b < 32 || b == 127)
        && !path[1..].split(|&b| b == b'/').any(|p| p.is_empty() || p == b"." || p == b"..")
}

fn uri(href: &str) -> Option<Vec<u8>> {
    if !href.get(..7)?.eq_ignore_ascii_case("file://") { return None; }
    let rest = &href[7..];
    let at = rest.find('/')?;
    if !rest[..at].is_empty() && !rest[..at].eq_ignore_ascii_case("localhost") { return None; }
    let raw = &rest[at..];
    if raw.contains(['?', '#']) { return None; }
    for (at, _) in raw.match_indices('%') {
        if !raw.as_bytes().get(at + 1..at + 3)?.iter().all(u8::is_ascii_hexdigit) { return None; }
    }
    let path = crate::paths::percent_decode_bytes(raw);
    local_path(&path).then_some(path)
}

fn stamp(text: &str) -> Option<(i64, u32)> {
    let bytes = text.as_bytes();
    if bytes.len() < 20 || [4, 7, 10, 13, 16].into_iter().zip(b"--T::").any(|(at, &b)| bytes[at] != b) { return None; }
    let number = |at, end| std::str::from_utf8(bytes.get(at..end)?).ok()?.parse::<i64>().ok();
    let (year, month, day, hour, minute, second) = (number(0, 4)?, number(5, 7)?, number(8, 10)?, number(11, 13)?, number(14, 16)?, number(17, 19)?);
    let leap = |year: i64| year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = [31, if leap(year) { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    if year < 1 || !(1..=12).contains(&month) || !(1..=days[(month - 1) as usize]).contains(&day)
        || !(0..24).contains(&hour) || !(0..60).contains(&minute) || !(0..60).contains(&second) { return None; }
    let mut rest = &text[19..];
    let mut nanos = 0;
    if let Some(fraction) = rest.strip_prefix('.') {
        let end = fraction.find(|c: char| !c.is_ascii_digit())?;
        if !(1..=9).contains(&end) { return None; }
        nanos = fraction[..end].parse::<u32>().ok()? * 10u32.pow(9 - end as u32);
        rest = &fraction[end..];
    }
    let offset = if rest == "Z" { 0 } else {
        let raw = rest.as_bytes();
        if raw.len() != 6 || !matches!(raw[0], b'+' | b'-') || raw[3] != b':' { return None; }
        let hour: i64 = rest[1..3].parse().ok()?;
        let minute: i64 = rest[4..6].parse().ok()?;
        if !(0..24).contains(&hour) || !(0..60).contains(&minute) { return None; }
        (hour * 60 + minute) * 60 * if raw[0] == b'+' { 1 } else { -1 }
    };
    let before = year - 1;
    let days = before * 365 + before / 4 - before / 100 + before / 400 + days[..(month - 1) as usize].iter().sum::<i64>() + day - 1;
    Some((days * 86400 + hour * 3600 + minute * 60 + second - offset, nanos))
}

#[cfg(test)]
#[path = "searchprior_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "searchprior_alloc_tests.rs"]
pub(super) mod alloc_tests;

#[cfg(test)]
#[path = "searchprior_xml_tests.rs"]
mod xml_tests;

#[cfg(test)]
#[path = "searchprior_session_tests.rs"]
mod session_tests;
