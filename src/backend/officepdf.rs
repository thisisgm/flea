// An office document becomes one cached PDF: LibreOffice runs headless in the jail Convert uses, one job at a time, and the PDF viewer draws the result.
use crate::backend::aliases::Aliases;
use crate::backend::archivework::{run_boxed_cancellable_limited, Work};
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
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
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
const PROGRAM: &str = "soffice";
// LibreOffice 26.8.1.1 converted nothing under 3.5 GiB of address space and every file at 4 GiB, so its jail gets twice that floor.
pub(crate) const ADDRESS_SPACE_BYTES: u64 = 8 * 1024 * 1024 * 1024;
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

// Sample output: soffice --headless --norestore --nolockcheck -env:UserInstallation=file:///tmp/flea-office-profile --convert-to pdf --outdir /out /in.docx
pub fn argv(program: &str, input: &Path, outdir: &Path) -> Vec<String> {
    let profile = format!("-env:UserInstallation={PROFILE_URL}");
    let fixed = [program, "--headless", "--norestore", "--nolockcheck", &profile, "--convert-to", "pdf", "--outdir"];
    let mut a: Vec<String> = fixed.iter().map(|s| s.to_string()).collect();
    a.push(outdir.to_string_lossy().into_owned());
    a.push(input.to_string_lossy().into_owned());
    a
}

pub enum Hit {
    Ready(PathBuf),
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
            return Hit::Failed(std::fs::read_to_string(&failed).unwrap_or_default());
        }
        let pdf = self.pdf_path(key);
        if Store::stamped(&pdf, mtime) { Hit::Ready(pdf) } else { Hit::Miss }
    }

    // AGENTS.md "Predictable path writes": unlink, exclusive create at 0600, then rename.
    fn publish(dest: &Path, mtime: SystemTime, body: &mut dyn Read) -> std::io::Result<()> {
        if let Some(parent) = dest.parent() {
            std::fs::DirBuilder::new().recursive(true).mode(0o700).create(parent)?;
        }
        let tmp = PathBuf::from(format!("{}.{}.tmp", dest.display(), std::process::id()));
        let _ = std::fs::remove_file(&tmp);
        let written = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp).and_then(|mut f| {
            std::io::copy(body, &mut f)?;
            f.set_modified(mtime)
        });
        match written.and_then(|()| std::fs::rename(&tmp, dest)) {
            Ok(()) => Ok(()),
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                Err(e)
            }
        }
    }

    fn record_failure(&self, key: &str, mtime: SystemTime, message: &str) {
        let dest = self.fail_path(key);
        if Store::publish(&dest, mtime, &mut message.as_bytes()).is_ok() {
            self.prune(dest.parent().unwrap_or(&self.dir));
        }
    }

    // The figure store's bound over one leaf, oldest conversion first: ctime is the publish time, the mtime being the source's.
    fn prune(&self, dir: &Path) {
        let cached = |n: &str| n.split_once('.').is_some_and(|(stem, ext)| stem.len() == KEY_HEX_CHARS && stem.bytes().all(|b| b.is_ascii_hexdigit()) && (ext == "pdf" || ext == "txt"));
        let age = |m: &std::fs::Metadata| SystemTime::UNIX_EPOCH + Duration::new(m.ctime().max(0) as u64, m.ctime_nsec() as u32);
        evict_leaf(dir, self.entries, self.bytes, &Names { entry: &cached, scratch: &|_| false, age: &age });
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
type Build = Box<dyn Fn(&str, &Path, &Path) -> Vec<String> + Send + Sync>;
// The jail the argv runs in, a seam so a test can stand in a converter without bwrap.
type Runner = Box<dyn Fn(Vec<String>, &Path, &mut Work, &AtomicBool) -> Result<(), FleaError> + Send + Sync>;

pub struct Service {
    store: Store,
    resolve: Resolve,
    build: Build,
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
        Service { store, resolve, build, runner, timeout, seq: AtomicU64::new(0), quit: Arc::new(AtomicBool::new(false)), queue: Mutex::new(Queue::default()) }
    }

    fn locked(&self) -> MutexGuard<'_, Queue> {
        self.queue.lock().unwrap_or_else(PoisonError::into_inner)
    }

    // Off the loop: the stat, the cache lookup and the conversion wait on a worker; reg is the quit registry id claimed on the loop.
    pub fn start(self: &Arc<Self>, wire: usize, reg: usize, path: PathBuf, tx: Sender<OpMsg>) {
        let (me, seq) = (Arc::clone(self), self.seq.fetch_add(1, Ordering::Relaxed));
        std::thread::spawn(move || me.request(wire, reg, seq, &path, &tx));
    }

    // Every terminal line leaves as DetachedDone, which frees reg; a miss first says converting and the end line follows.
    fn request(self: &Arc<Self>, wire: usize, reg: usize, seq: u64, path: &Path, tx: &Sender<OpMsg>) {
        let end = |line: String| drop(tx.send(OpMsg::DetachedDone { id: reg, line }));
        let Some(program) = (self.resolve)() else {
            return end(failed_line(wire, "LibreOffice is not installed on this box.", true));
        };
        let (store, source) = (self.store.clone(), path.to_path_buf());
        let probed = iomount::call(path, &iomount::mount_body(), "office", move || {
            let io = |e: std::io::Error| crate::error::io_message(&e);
            let input = std::fs::canonicalize(&source).map_err(io)?;
            let mtime = std::fs::metadata(&input).and_then(|m| m.modified()).map_err(io)?;
            let key = Store::key(&source);
            let hit = store.lookup(&key, mtime);
            Ok::<_, String>((mtime, key, hit, input))
        });
        let (mtime, key, hit, input) = match probed {
            Ok(Ok(found)) => found,
            Ok(Err(message)) => return end(failed_line(wire, &message, false)),
            Err(e) => return end(failed_line(wire, &e.msg, false)),
        };
        match hit {
            Hit::Ready(pdf) => return end(ready_line(wire, &pdf)),
            Hit::Failed(message) => return end(failed_line(wire, &message, false)),
            Hit::Miss => {}
        }
        let _ = tx.send(OpMsg::Meta { line: converting_line(wire) });
        self.enqueue(Job { key, program, input, mtime, seq, waiters: vec![Waiter { wire, reg, tx: tx.clone() }], required: false, cancel: None });
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
            return answer(job.waiters, Err("replaced by a newer request".to_string()));
        }
        if let Some(old) = q.queued.take() {
            let (mut keep, lose) = if job.seq > old.seq { (job, old) } else { (old, job) };
            if keep.key == lose.key {
                keep.waiters.extend(lose.waiters);
            } else {
                answer(lose.waiters, Err("replaced by a newer request".to_string()));
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
            let outcome = self.convert(&job);
            let mut q = self.locked();
            let done = q.running.take().unwrap_or_default();
            let mut next = q.required.pop_front().or_else(|| q.queued.take());
            if let Some(n) = next.as_mut() {
                q.running = Some(std::mem::take(&mut n.waiters));
                if !n.required { q.started = n.seq; }
            }
            drop(q);
            answer(done, outcome);
            match next {
                Some(n) => job = n,
                None => break,
            }
        }
    }

    // A hit or a recorded failure made by another window since the request is served, so one file and mtime convert once.
    fn convert(&self, job: &Job) -> Result<PathBuf, String> {
        if self.quit.load(Ordering::Relaxed) || job.cancel.as_ref().is_some_and(|c| c.load(Ordering::Relaxed)) {
            return Err("cancelled".to_string());
        }
        match self.store.lookup(&job.key, job.mtime) {
            Hit::Ready(pdf) => return Ok(pdf),
            Hit::Failed(message) => return Err(message),
            Hit::Miss => {}
        }
        match self.run(job) {
            Ok(pdf) => Ok(pdf),
            Err(Fail::Local(message)) => Err(message),
            Err(Fail::Converter(message)) => {
                self.store.record_failure(&job.key, job.mtime, &message);
                Err(message)
            }
        }
    }

    fn run(&self, job: &Job) -> Result<PathBuf, Fail> {
        let local = |e: std::io::Error| Fail::Local(crate::error::io_message(&e));
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&self.store.dir).map_err(local)?;
        let mut work = Work::new(&self.store.dir, "office").map_err(|e| Fail::Local(e.msg))?;
        let argv = (self.build)(&job.program, &job.input, &work.dir);
        let (stop, finished, timed_out) = (AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false));
        let ran = std::thread::scope(|s| {
            s.spawn(|| {
                let end = Instant::now() + self.timeout;
                while !finished.load(Ordering::Relaxed) {
                    let late = Instant::now() >= end;
                    if late || self.quit.load(Ordering::Relaxed) || job.cancel.as_ref().is_some_and(|c| c.load(Ordering::Relaxed)) {
                        timed_out.store(late, Ordering::Relaxed);
                        stop.store(true, Ordering::Relaxed);
                        break;
                    }
                    std::thread::sleep(WATCH_STEP);
                }
            });
            let ran = (self.runner)(argv, &job.input, &mut work, &stop);
            finished.store(true, Ordering::Relaxed);
            ran
        });
        if let Err(e) = ran {
            return Err(if timed_out.load(Ordering::Relaxed) {
                Fail::Local(format!("the conversion took longer than {} seconds", self.timeout.as_secs()))
            } else if self.quit.load(Ordering::Relaxed) || job.cancel.as_ref().is_some_and(|c| c.load(Ordering::Relaxed)) {
                Fail::Local("cancelled".to_string())
            } else {
                Fail::Converter(e.msg)
            });
        }
        let produced = produced_pdf(&work.dir).ok_or_else(|| Fail::Converter(format!("{PROGRAM} wrote no PDF of {}", job.input.display())))?;
        let dest = self.store.pdf_path(&job.key);
        let mut body = std::fs::File::open(&produced).map_err(local)?;
        Store::publish(&dest, job.mtime, &mut body).map_err(local)?;
        self.store.prune(&self.store.dir);
        Ok(dest)
    }
}

// Only the converter's own failure is remembered for the file; a timeout, a local error or a cancel fails this request and leaves no marker.
enum Fail {
    Converter(String),
    Local(String),
}

// The one regular, non-empty PDF the converter left in the work folder.
fn produced_pdf(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| {
        p.extension().is_some_and(|x| x == "pdf") && std::fs::symlink_metadata(p).is_ok_and(|m| m.is_file() && m.len() > 0)
    })
}

// Every waiter on the job gets the end line, which also removes its id from the quit registry.
fn answer(waiters: Vec<Waiter>, outcome: Result<PathBuf, String>) {
    for w in waiters {
        let line = match &outcome {
            Ok(pdf) => ready_line(w.wire, pdf),
            Err(message) => failed_line(w.wire, message, false),
        };
        let _ = w.tx.send(OpMsg::DetachedDone { id: w.reg, line });
    }
}

// The production runner: the convert jail with the office address-space cap.
pub(crate) fn jail(argv: Vec<String>, input: &Path, work: &mut Work, stop: &AtomicBool) -> Result<(), FleaError> {
    run_boxed_cancellable_limited("office", argv, input, work, stop, sandbox::CPU_SECONDS, ADDRESS_SPACE_BYTES)
}

fn production() -> &'static Arc<Service> {
    static SERVICE: OnceLock<Arc<Service>> = OnceLock::new();
    SERVICE.get_or_init(|| {
        let resolve: Resolve = Box::new(|| {
            if sandbox::available() { find_program(PROGRAM, &std::env::var("PATH").unwrap_or_default()) } else { None }
        });
        Arc::new(Service::new(Store::new(&crate::userfile::cache_home()), resolve, Box::new(argv), Box::new(jail), CONVERT_TIMEOUT))
    })
}

// Sample input: {"c":"officepdf","id":4,"path":"/home/gm/Documents/report.docx"}.
pub fn serve(out: &mut impl Write, ops: &mut Ops, mime: &Db, aliases: &Aliases, id: usize, path: &str) {
    if !is_office(path, mime, aliases) {
        writeln!(out, "{}", failed_line(id, "that file is not an office document", true)).ok();
        out.flush().ok();
        return;
    }
    let service = production();
    let reg = ops.claim_id();
    ops.detached.insert(reg, &service.quit);
    service.start(id, reg, PathBuf::from(path), ops.tx.clone());
}

#[cfg(test)]
#[path = "officepdf_tests.rs"]
mod tests;

#[path = "officepdf_fetch.rs"]
mod fetch;
pub use fetch::cached_pdf;

#[path = "officepdf_lines.rs"]
mod lines;
pub use lines::{converting_line, failed_line, ready_line};
