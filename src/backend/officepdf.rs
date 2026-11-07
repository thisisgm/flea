// An office document becomes one cached PDF: LibreOffice runs headless in the jail Convert uses, one job at a time, and the PDF viewer draws the result.
use crate::backend::aliases::Aliases;
use crate::backend::archivework::{run_boxed_office, Work};
use crate::backend::iomount;
use crate::backend::md5;
use crate::backend::mime::Db;
use crate::backend::opsdispatch::Ops;
use crate::backend::opsreq::OpMsg;
use crate::backend::sandbox;
use crate::backend::thumbcache::uri_for;
use crate::error::FleaError;
use crate::figurestore::{evict_leaf, Names};
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant, SystemTime};

// Documents, spreadsheets and presentations, by canonical MIME name.
const OFFICE_TYPES: [&str; 10] = [
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    "application/vnd.oasis.opendocument.text",
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    "application/vnd.oasis.opendocument.spreadsheet",
    "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    "application/rtf",
    "application/vnd.oasis.opendocument.presentation",
    "application/msword",
    "application/vnd.ms-excel",
    "application/vnd.ms-powerpoint",
];
const CACHE_LEAF: &str = "flea/office";
// The failure markers keep the thumbnail cache's fail/flea/ shape, beside the PDFs and never in the shared thumbnails tree.
const FAIL_LEAF: &str = "fail/flea";
// Bump this version when the conversion argv or jail profile changes; cached PDFs stay reusable.
const FAILURE_PROFILE: &str = "flea-office-profile-v4\n";
const PROGRAM: &str = "soffice";
// The jail's own tmpfs, so the profile LibreOffice writes needs no bind.
const PROFILE_URL: &str = "file:///tmp/flea-office-profile";
pub const CONVERT_TIMEOUT: Duration = Duration::from_secs(60);
const WATCH_STEP: Duration = Duration::from_millis(50);
const ANY_EXECUTE_BIT: u32 = 0o111;
// The bound the figure store keeps, over the PDFs and over the failure markers each.
const MAX_ENTRIES: usize = 512;
const MAX_BYTES: u64 = 512 * 1024 * 1024;
const KEY_HEX_CHARS: usize = 32;

pub fn is_office(name: &str, mime: &Db, aliases: &Aliases) -> bool {
    mime.lookup(name).is_some_and(|m| OFFICE_TYPES.contains(&aliases.canonical(m)))
}

// A program counts only when it resolves to a regular executable under /usr, as a thumbnailer does.
fn find_program(name: &str, path: &str) -> Option<String> {
    let runs = |candidate: &Path| {
        std::fs::canonicalize(candidate).is_ok_and(|real| {
            real.starts_with("/usr") && std::fs::metadata(&real).is_ok_and(|m| m.is_file() && m.permissions().mode() & ANY_EXECUTE_BIT != 0)
        })
    };
    path.split(':').filter(|d| !d.is_empty()).map(|d| Path::new(d).join(name)).find(|c| runs(c)).map(|c| c.to_string_lossy().into_owned())
}

#[path = "officepublish.rs"]
mod publication;
#[path = "officepdf_run.rs"]
mod running;
use running::Fail;
#[cfg(test)]
use running::produced_pdf;
#[path = "officesheets.rs"]
mod sheets;
pub use sheets::{is_sheet, sheets_argv, sheets_line, Sheets, SHEETS_SUFFIX};
#[path = "officehead.rs"]
mod head;
#[cfg(test)]
pub use head::argv;
use head::{convert_argv, head_extension, head_target, HEAD_SUFFIX};

pub enum Hit<T = PathBuf> {
    Ready(T),
    Failed(String),
    Miss,
}

// The PDFs and the failure markers, each stamped with the source's mtime so a changed file converts again.
#[derive(Clone)]
pub struct Store {
    dir: PathBuf,
    entries: usize,
    bytes: u64,
}

impl Store {
    pub fn new(cache: &Path) -> Store {
        Store { dir: cache.join(CACHE_LEAF), entries: MAX_ENTRIES, bytes: MAX_BYTES }
    }

    pub fn key(path: &Path) -> String {
        md5::hex(uri_for(path).as_bytes())
    }

    fn pdf_path(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{key}.pdf"))
    }

    fn fail_path(&self, key: &str) -> PathBuf {
        self.dir.join(FAIL_LEAF).join(format!("{key}.txt"))
    }

    // A planted symlink is not a regular file, so it is neither served nor read.
    fn stamped(path: &Path, mtime: SystemTime) -> bool {
        std::fs::symlink_metadata(path).is_ok_and(|m| m.is_file() && m.modified().is_ok_and(|t| t == mtime))
    }

    pub fn lookup(&self, key: &str, mtime: SystemTime) -> Hit {
        let failed = self.fail_path(key);
        if Store::stamped(&failed, mtime) {
            if let Ok(body) = std::fs::read_to_string(&failed) {
                if let Some(message) = body.strip_prefix(FAILURE_PROFILE) {
                    return Hit::Failed(message.to_string());
                }
            }
        }
        let pdf = self.pdf_path(key);
        if Store::stamped(&pdf, mtime) { Hit::Ready(pdf) } else { Hit::Miss }
    }

    fn publish(dest: &Path, mtime: SystemTime, body: &mut dyn Read) -> std::io::Result<()> {
        publication::publish(dest, mtime, body)
    }

    fn record_failure(&self, key: &str, mtime: SystemTime, message: &str) {
        let dest = self.fail_path(key);
        let body = format!("{FAILURE_PROFILE}{message}");
        if Store::publish(&dest, mtime, &mut body.as_bytes()).is_ok() {
            self.prune(dest.parent().unwrap_or(&self.dir));
        }
    }

    // The figure store's bound over one leaf, oldest conversion first: ctime is the publish time, the mtime being the source's.
    fn prune(&self, dir: &Path) {
        let ours = |ext: &str| matches!(ext, "pdf" | "txt" | "sheets.txt") || head_extension(ext) || ext.strip_prefix("sheets.").and_then(|e| e.strip_suffix(".csv")).is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
        let cached = |n: &str| n.split_once('.').is_some_and(|(stem, ext)| stem.len() == KEY_HEX_CHARS && stem.bytes().all(|b| b.is_ascii_hexdigit()) && ours(ext));
        let age = |m: &std::fs::Metadata| SystemTime::UNIX_EPOCH + Duration::new(m.ctime().max(0) as u64, m.ctime_nsec() as u32);
        evict_leaf(dir, self.entries, self.bytes, &Names { entry: &|name| cached(name) && crate::cachewrite::allowed(&dir.join(name)), scratch: &|_| false, age: &age });
    }
}

struct Waiter {
    wire: usize,
    reg: usize,
    tx: Sender<OpMsg>,
}

struct Job {
    key: String,
    program: String,
    // Canonical, resolved by the probe under the mount deadline, so the conversion makes no path call of its own.
    input: PathBuf,
    mtime: SystemTime,
    seq: u64,
    waiters: Vec<Waiter>,
    required: bool,
    // The workbook's sheets as CSVs instead of a PDF; its key carries SHEETS_SUFFIX.
    sheets: bool,
    // A preview's export filter: the first PREVIEW_PAGES pages only, under a key that carries HEAD_SUFFIX.
    head: Option<&'static str>,
    cancel: Option<Arc<AtomicBool>>,
}

#[derive(Default)]
struct Queue {
    // The running job's waiters, and the newest sequence started, which the job's end does not clear: an older request that lands late is answered against it.
    running: Option<Vec<Waiter>>,
    started: u64,
    queued: Option<Job>,
    required: std::collections::VecDeque<Job>,
}

type Resolve = Box<dyn Fn() -> Option<String> + Send + Sync>;
// The program, the --convert-to target, the input and the output folder.
type Build = Box<dyn Fn(&str, &str, &Path, &Path) -> Vec<String> + Send + Sync>;
type SheetBuild = Box<dyn Fn(&str, &Path, &Path, i32) -> Vec<String> + Send + Sync>;
// The jail the argv runs in, a seam so a test can stand in a converter without bwrap.
type Runner = Box<dyn Fn(Vec<String>, &Path, &mut Work, &AtomicBool) -> Result<(), FleaError> + Send + Sync>;

pub struct Service {
    store: Store,
    resolve: Resolve,
    build: Build,
    sheet_build: SheetBuild,
    runner: Runner,
    timeout: Duration,
    // Request order, stamped on the loop: a worker that finishes its probe late must not replace a newer request.
    seq: AtomicU64,
    // One flag for every request: the shutdown sets it through the quit registry and every job stops.
    quit: Arc<AtomicBool>,
    queue: Mutex<Queue>,
}

impl Service {
    pub fn new(store: Store, resolve: Resolve, build: Build, runner: Runner, timeout: Duration) -> Service {
        Service { store, resolve, build, sheet_build: Box::new(sheets_argv), runner, timeout, seq: AtomicU64::new(0), quit: Arc::new(AtomicBool::new(false)), queue: Mutex::new(Queue::default()) }
    }

    fn locked(&self) -> MutexGuard<'_, Queue> {
        self.queue.lock().unwrap_or_else(PoisonError::into_inner)
    }

    // Off the loop: the stat, the cache lookup and the conversion wait on a worker; reg is the quit registry id claimed on the loop.
    pub fn start(self: &Arc<Self>, wire: usize, reg: usize, path: PathBuf, sheets: bool, head: Option<&'static str>, tx: Sender<OpMsg>) {
        let (me, seq) = (Arc::clone(self), self.seq.fetch_add(1, Ordering::Relaxed));
        std::thread::spawn(move || me.request(wire, reg, seq, &path, sheets, head, &tx));
    }

    // Every terminal line leaves as DetachedDone, which frees reg; a miss first says converting and the end line follows.
    #[allow(clippy::too_many_arguments)]
    fn request(self: &Arc<Self>, wire: usize, reg: usize, seq: u64, path: &Path, sheets: bool, head: Option<&'static str>, tx: &Sender<OpMsg>) {
        let end = |line: String| drop(tx.send(OpMsg::DetachedDone { id: reg, line }));
        let Some(program) = (self.resolve)() else {
            return end(failed_line_of(sheets, wire, "LibreOffice is not installed on this box.", true));
        };
        let (store, source) = (self.store.clone(), path.to_path_buf());
        let probed = iomount::call(path, &iomount::mount_body(), "office", move || {
            let io = |e: std::io::Error| crate::error::io_message(&e);
            let input = std::fs::canonicalize(&source).map_err(io)?;
            let mtime = std::fs::metadata(&input).and_then(|m| m.modified()).map_err(io)?;
            let suffix = if sheets { SHEETS_SUFFIX } else if head.is_some() { HEAD_SUFFIX } else { "" };
            let key = format!("{}{suffix}", Store::key(&source));
            let hit = store.find(&key, mtime, sheets);
            Ok::<_, String>((mtime, key, hit, input))
        });
        let (mtime, key, hit, input) = match probed {
            Ok(Ok(found)) => found,
            Ok(Err(message)) => return end(failed_line_of(sheets, wire, &message, false)),
            Err(e) => return end(failed_line_of(sheets, wire, &e.msg, false)),
        };
        match hit {
            Hit::Ready(done) => return end(done_line(wire, &done)),
            Hit::Failed(message) => return end(failed_line_of(sheets, wire, &message, false)),
            Hit::Miss => {}
        }
        let _ = tx.send(OpMsg::Meta { line: converting_line_of(sheets, wire) });
        self.enqueue(Job { key, program, input, mtime, seq, waiters: vec![Waiter { wire, reg, tx: tx.clone() }], required: false, sheets, head, cancel: None });
    }

    // The running job is never replaced; a request older than the newest started is answered replaced, as is the lower sequence of two queued, and the same file shares one job.
    fn enqueue(self: &Arc<Self>, mut job: Job) {
        let mut q = self.locked();
        if job.required {
            if q.running.is_some() { q.required.push_back(job); return; }
            q.running = Some(std::mem::take(&mut job.waiters));
            drop(q);
            let me = Arc::clone(self);
            std::thread::spawn(move || me.work(job));
            return;
        }
        if job.seq < q.started {
            return answer(job.waiters, job.sheets, Err("replaced by a newer request".to_string()));
        }
        if let Some(old) = q.queued.take() {
            let (mut keep, lose) = if job.seq > old.seq { (job, old) } else { (old, job) };
            if keep.key == lose.key {
                keep.waiters.extend(lose.waiters);
            } else {
                answer(lose.waiters, lose.sheets, Err("replaced by a newer request".to_string()));
            }
            job = keep;
        }
        if q.running.is_some() {
            q.queued = Some(job);
            return;
        }
        q.running = Some(std::mem::take(&mut job.waiters));
        q.started = job.seq;
        let me = Arc::clone(self);
        std::thread::spawn(move || me.work(job));
    }

    fn work(self: Arc<Self>, mut job: Job) {
        loop {
            let (outcome, sheets) = (self.convert(&job), job.sheets);
            let mut q = self.locked();
            let done = q.running.take().unwrap_or_default();
            let mut next = q.required.pop_front().or_else(|| q.queued.take());
            if let Some(n) = next.as_mut() {
                q.running = Some(std::mem::take(&mut n.waiters));
                if !n.required { q.started = n.seq; }
            }
            drop(q);
            answer(done, sheets, outcome);
            match next {
                Some(n) => job = n,
                None => break,
            }
        }
    }

    // A hit or a recorded failure made by another window since the request is served, so one file and mtime convert once.
    fn convert(&self, job: &Job) -> Result<Done, String> {
        if self.quit.load(Ordering::Relaxed) || job.cancel.as_ref().is_some_and(|c| c.load(Ordering::Relaxed)) {
            return Err("cancelled".to_string());
        }
        match self.store.find(&job.key, job.mtime, job.sheets) {
            Hit::Ready(done) => return Ok(done),
            Hit::Failed(message) => return Err(message),
            Hit::Miss => {}
        }
        match if job.sheets { self.run_sheets(job).map(Done::Sheets) } else { self.run(job).map(Done::Pdf) } {
            Ok(done) => Ok(done),
            Err(Fail::Local(message)) => Err(message),
            Err(Fail::Converter(message)) => {
                self.store.record_failure(&job.key, job.mtime, &message);
                Err(message)
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Done {
    Pdf(PathBuf),
    Sheets(Sheets),
}

fn done_line(id: usize, done: &Done) -> String {
    match done {
        Done::Pdf(pdf) => ready_line(id, pdf),
        Done::Sheets(sheets) => sheets_line(id, sheets),
    }
}

// Every waiter on the job gets the end line, which also removes its id from the quit registry.
fn answer(waiters: Vec<Waiter>, sheets: bool, outcome: Result<Done, String>) {
    for w in waiters {
        let line = match &outcome {
            Ok(done) => done_line(w.wire, done),
            Err(message) => failed_line_of(sheets, w.wire, message, false),
        };
        let _ = w.tx.send(OpMsg::DetachedDone { id: w.reg, line });
    }
}

// The production runner: the convert jail with the office address-space cap.
pub(crate) fn jail(argv: Vec<String>, input: &Path, work: &mut Work, stop: &AtomicBool) -> Result<(), FleaError> {
    run_boxed_office(argv, input, work, stop)
}

// The production service over one cache root; the shared instance below uses the user's cache.
fn service(cache: &Path) -> Arc<Service> {
    let resolve: Resolve = Box::new(|| {
        if sandbox::available() { find_program(PROGRAM, &std::env::var("PATH").unwrap_or_default()) } else { None }
    });
    Arc::new(Service::new(Store::new(cache), resolve, Box::new(convert_argv), Box::new(jail), CONVERT_TIMEOUT))
}

fn production() -> &'static Arc<Service> {
    static SERVICE: OnceLock<Arc<Service>> = OnceLock::new();
    SERVICE.get_or_init(|| service(&crate::userfile::cache_home()))
}

// Sample input: {"c":"officepdf","id":4,"path":"/home/gm/Documents/report.docx"}; officesheets asks a spreadsheet's sheets.
pub fn serve(out: &mut impl Write, ops: &mut Ops, mime: &Db, aliases: &Aliases, id: usize, path: &str, sheets: bool) {
    if !is_office(path, mime, aliases) || (sheets && !is_sheet(path, mime, aliases)) {
        let refusal = if sheets { "that file is not a spreadsheet" } else { "that file is not an office document" };
        writeln!(out, "{}", failed_line_of(sheets, id, refusal, true)).ok();
        out.flush().ok();
        return;
    }
    let service = production();
    let reg = ops.claim_id();
    ops.detached.insert(reg, &service.quit);
    let head = (!sheets).then(|| head::export_filter(path, mime, aliases));
    service.start(id, reg, PathBuf::from(path), sheets, head, ops.tx.clone());
}

#[cfg(test)]
#[path = "officepdf_tests.rs"]
mod tests;

#[path = "officepdf_fetch.rs"]
mod fetch;
pub use fetch::cached_pdf;
#[cfg(test)]
pub(crate) use fetch::cached_pdf_in;

#[path = "officepdf_lines.rs"]
mod lines;
pub use lines::{converting_line_of, failed_line_of, ready_line};
#[cfg(test)]
pub use lines::{converting_line, failed_line};
