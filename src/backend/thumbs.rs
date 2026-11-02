use crate::backend::aliases::Aliases;
use crate::backend::appicon;
use crate::backend::child::{run_with_timeout, Ran};
use crate::backend::{rawpreview, sandbox, thumbbind};
use crate::backend::thumbargv::argv;
use crate::backend::thumbcache::{uri_for, Cache};
use crate::backend::thumbspec::Thumbnailers;
use crate::backend::thumbwrite::{exclusive_temp, stamp, write_marker};
use crate::backend::workerlink::{worker_shape, WorkerLink};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

// A viewport holds about 35 rows, so a queue twice that absorbs one fast scroll without unbounded growth.
pub const MAX_QUEUE: usize = 70;
// The freedesktop "large" size, which is what this box's cache holds.
pub const THUMB_SIZE: u32 = 256;
// A decoder that has not answered in this long is hung, and a hung child starves the pool.
pub(crate) const JOB_TIMEOUT: Duration = Duration::from_secs(20);

pub struct Job {
    pub replies: Option<super::thumbreq::identity::Shared>,
    pub path: PathBuf,
    pub mtime: i64,
    // The submitted Reviewed identity for Trash; unknown listing and shelf identities stay None.
    pub trash_identity: Option<String>,
    pub mime: String,
    // None unless FLEA_THUMB_TRACE is set, so an untraced job carries no marks; see AGENTS.md "Thumbnail trace".
    pub trace: Option<Trace>,
}

// Every mark is a delta from the one Instant taken at submit, so the four durations sum to the job's whole life.
pub struct Trace {
    pub row: usize,
    pub depth: usize,
    pub at: Instant,
    pub popped: Duration,
    pub spawned: Duration,
    pub exited: Duration,
}

// The environment is read once for the process, because the queue path must not pay a lookup per row.
pub fn trace(row: usize) -> Option<Trace> {
    static ON: OnceLock<bool> = OnceLock::new();
    if !*ON.get_or_init(|| std::env::var_os("FLEA_THUMB_TRACE").is_some_and(|v| !v.is_empty())) {
        return None;
    }
    let zero = Duration::ZERO;
    Some(Trace { row, depth: 0, at: Instant::now(), popped: zero, spawned: zero, exited: zero })
}

pub enum Outcome {
    Ready(PathBuf),
    Failed,
}

pub struct Done {
    pub replies: Option<super::thumbreq::identity::Shared>,
    pub path: PathBuf,
    pub mtime: i64,
    pub trash_identity: Option<String>,
    pub result: Outcome,
    pub ms: f64,
    pub trace: Option<Trace>,
}

// Parsed once by the caller and shared from here, so no pool thread reads these files again; the shelf's single job has no pool and so no worker.
pub(crate) struct Tables { pub aliases: Arc<Aliases>, pub specs: Arc<Thumbnailers>, pub cache: Cache, pub worker: Option<WorkerLink> }

type Shared = Arc<(Mutex<VecDeque<Job>>, Condvar)>;

pub struct Pool {
    inner: Shared,
    startup: super::lazy::Lazy<()>,
}

impl Pool {
    // The cache root and both tables are the caller's, so tests share parsed tables without writing into the operator's cache.
    #[cfg(test)]
    pub fn new(workers: usize, results: Sender<Done>, root: PathBuf, aliases: Arc<Aliases>, specs: Arc<Thumbnailers>) -> Pool {
        Pool::start(workers, results, Tables { aliases, specs, cache: Cache::at(root), worker: Some(WorkerLink::new()) })
    }

    #[cfg(test)]
    fn start(workers: usize, results: Sender<Done>, tables: Tables) -> Pool {
        let inner: Shared = Arc::new((Mutex::new(VecDeque::new()), Condvar::new()));
        Self::start_workers(&inner, workers, results, tables);
        Pool { inner, startup: ().into() }
    }

    fn start_workers(inner: &Shared, workers: usize, results: Sender<Done>, tables: Tables) {
        let tables = Arc::new(tables);
        for _ in 0..workers.max(1) {
            let (inner, tables, results) = (Arc::clone(inner), Arc::clone(&tables), results.clone());
            std::thread::spawn(move || worker(inner, results, tables));
        }
    }

    // Tables and workers are demanded only by the first uncached thumbnail job.
    pub(crate) fn lazy(workers: usize, tables: &super::state::Tables, root: PathBuf, events: super::events::Sender<super::events::Event>) -> Pool {
        let inner: Shared = Arc::new((Mutex::new(VecDeque::new()), Condvar::new()));
        let shared = Arc::clone(&inner);
        let (aliases, specs) = (tables.aliases.clone(), tables.thumbs.clone());
        let startup = super::lazy::Lazy::new(move || {
            let (results, done) = std::sync::mpsc::channel();
            super::events::spawn_forwarder(done, events);
            if !sandbox::available() {
                eprintln!("flea: thumbnails are disabled, bwrap or prlimit is not on PATH");
            }
            let tables = Tables { aliases: Arc::clone(&aliases), specs: Arc::clone(&specs), cache: Cache::at(root), worker: Some(WorkerLink::new()) };
            Self::start_workers(&shared, workers, results, tables);
        });
        Pool { inner, startup }
    }

    // Returns the jobs it dropped to make room, so a caller can unmap and answer the rows that will now never report.
    pub fn submit(&self, mut job: Job) -> Vec<Job> {
        let _ = &*self.startup;
        let (lock, cv) = &*self.inner;
        let mut q = lock.lock().unwrap();
        let mut dropped = Vec::new();
        // The oldest job is the one furthest from the viewport, so it is the one to drop.
        while q.len() >= MAX_QUEUE {
            match q.pop_front() {
                Some(j) => dropped.push(j),
                None => break,
            }
        }
        // The depth at submit is the number of jobs already ahead of this one, which is what says whether the workers were starved.
        if let Some(t) = job.trace.as_mut() {
            t.depth = q.len();
        }
        q.push_back(job);
        cv.notify_one();
        dropped
    }

    // Returns the queued jobs it removed; a job already inside a worker is never one of them and still reports.
    pub fn cancel(&self, path: &Path, trash_identity: Option<&str>) -> Vec<Job> {
        let (lock, _cv) = &*self.inner;
        let mut q = lock.lock().unwrap();
        let (dropped, kept): (Vec<Job>, Vec<Job>) = q.drain(..).partition(|j| j.path == path && j.trash_identity.as_deref() == trash_identity);
        *q = kept.into();
        dropped
    }

    // Ordinary cancellation follows queued job identities, including jobs whose row mapping was already consumed.
    pub fn cancel_listing(&self) -> Vec<Job> {
        let (lock, _cv) = &*self.inner;
        let mut q = lock.lock().unwrap();
        let (dropped, kept): (Vec<Job>, Vec<Job>) = q.drain(..).partition(|j| j.trash_identity.is_none());
        *q = kept.into();
        dropped
    }

    // Taken and cleared under one lock, because a caller that read the queue first could race a worker's own pop.
    pub fn cancel_all(&self) -> Vec<Job> {
        let (lock, _cv) = &*self.inner;
        let mut q = lock.lock().unwrap();
        q.drain(..).collect()
    }

    // Test-only: a pool that starts no worker, so a request-policy test reads back exactly what it queued and a drain waits out its limit; a harness that needs jobs run attaches its own worker to `inner`.
    #[cfg(test)]
    pub(crate) fn idle() -> Pool {
        Pool { inner: Arc::new((Mutex::new(VecDeque::new()), Condvar::new())), startup: ().into() }
    }

    // Jobs no worker has taken yet; the request policy reads it to see how many more the queue holds.
    pub fn pending(&self) -> usize {
        let (lock, _cv) = &*self.inner;
        lock.lock().unwrap().len()
    }
}

fn worker(inner: Shared, results: Sender<Done>, tables: Arc<Tables>) {
    loop {
        let mut job = {
            let (lock, cv) = &*inner;
            let mut q = lock.lock().unwrap();
            while q.is_empty() {
                q = cv.wait(q).unwrap();
            }
            match q.pop_front() {
                Some(j) => j,
                None => continue,
            }
        };
        if let Some(t) = job.trace.as_mut() {
            t.popped = t.at.elapsed();
        }
        let started = Instant::now();
        let outcome = run_one(&tables, &mut job);
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        if results.send(Done { replies: job.replies, path: job.path, mtime: job.mtime, trash_identity: job.trash_identity, result: outcome, ms, trace: job.trace }).is_err() {
            return;
        }
    }
}

// pub(crate) so the shelf can run one job in its own thread: a bar widget has no listing and no pool.
pub(crate) fn run_one(tables: &Tables, job: &mut Job) -> Outcome {
    run_one_in(tables, job, &rawpreview::cache_dir())
}

// The copies of a camera RAW's embedded JPEG live in raw_dir, which a test points at its sandbox instead of the user's cache.
pub(crate) fn run_one_in(tables: &Tables, job: &mut Job, raw_dir: &Path) -> Outcome {
    if super::model3d::embedded_thumbnail_supported(&job.path.to_string_lossy()) { return super::model3dreq::thumbnail(tables,job) }
    let spec = match tables.specs.for_mime(&job.mime, &tables.aliases) {
        Some(s) => s,
        None => return Outcome::Failed,
    };
    if appicon::is_route(spec) {
        return appicon::run(tables, job);
    }
    // The cache key is the URI of the path the user named, because that is the key every other application on the box looks under; see AGENTS.md "Thumbnail cache".
    let key_uri = uri_for(&job.path);
    let final_path = tables.cache.large_path_for(&key_uri, job.trash_identity.as_deref());
    // Checked before the thumbnailer runs, so a document this cache cannot replace costs no child; publish checks again.
    if !crate::cachewrite::allowed(&final_path) {
        return Outcome::Failed;
    }
    // A RAW that holds no preview fails here, unmarked, because the walk is cheap and an unwritable cache may be the cause.
    let input = match input_for(tables, job, raw_dir) {
        Some(p) => p,
        None => return Outcome::Failed,
    };
    let dir = match final_path.parent() {
        Some(d) => d.to_path_buf(),
        None => return Outcome::Failed,
    };
    if std::fs::create_dir_all(&dir).is_err() {
        return Outcome::Failed;
    }
    let temp = match exclusive_temp(&dir) {
        Some(t) => t,
        None => return Outcome::Failed,
    };
    // corner: an input that will not canonicalise is not recorded in fail/, because a vanished file is not a broken one; see AGENTS.md "Thumbnail pool".
    let (abs, inner) = match argv(spec, &input, &temp, THUMB_SIZE) {
        Some(a) => a,
        None => return discard(&temp),
    };
    // corner: a missing bwrap or prlimit, or a program the jail cannot bind safely, fails the job closed and records nothing, because neither is a broken file; see AGENTS.md "Thumbnail sandbox".
    if !sandbox::available() {
        return discard(&temp);
    }
    // corner: a thumbnailer that wrote then renamed would fail against this file bind, and only glycin and ffmpegthumbnailer were probed; see AGENTS.md "Thumbnail pool".
    let Some(full) = thumbbind::wrap(&inner, &abs, &temp) else { return discard(&temp) };
    if let Some(t) = job.trace.as_mut() {
        t.spawned = t.at.elapsed();
    }
    // A video the pre-linked worker can take goes there first; anything it does not finish runs the exec path as before.
    let by_worker = match (&tables.worker, worker_shape(spec)) {
        (Some(worker), Some(film_strip)) => worker.generate(&abs, &temp, THUMB_SIZE, film_strip, JOB_TIMEOUT),
        _ => None,
    };
    let ran = match by_worker {
        Some(ran) => ran,
        None => run_with_timeout(&full, JOB_TIMEOUT),
    };
    if let Some(t) = job.trace.as_mut() {
        t.exited = t.at.elapsed();
    }
    if matches!(ran, Ran::Succeeded) && wrote_something(&temp) {
    } else if records_marker(&ran, super::extclass::classify(&job.path)) {
        // corner: glycin exits 0 on bytes it cannot decode, so an empty output is the decoder's verdict on the file; see AGENTS.md "Thumbnail pool".
        record_failure_for(&tables.cache, &key_uri, job.mtime, tables.specs.fingerprint(), job.trash_identity.as_deref());
        return discard(&temp);
    } else {
        // corner: a child that never started is the machine's fault, not the file's, so it is not recorded in fail/; see AGENTS.md "Thumbnail pool".
        return discard(&temp);
    }
    publish(&temp, &key_uri, job.mtime, final_path)
}

// The child wrote a bare PNG, so the spec's own metadata is added before the file is published.
pub(crate) fn publish(temp: &Path, key_uri: &str, mtime: i64, final_path: PathBuf) -> Outcome {
    if crate::cachewrite::require(&final_path).is_err() { return discard(temp); }
    if stamp(temp, key_uri, mtime).is_err() || std::fs::rename(temp, &final_path).is_err() {
        return discard(temp);
    }
    Outcome::Ready(final_path)
}

// What the thumbnailer is handed: a camera RAW's embedded JPEG, copied out once per file version, or the file itself.
pub(crate) fn input_for(tables: &Tables, job: &Job, raw_dir: &Path) -> Option<PathBuf> {
    match tables.specs.raw_route(&job.mime, &tables.aliases) {
        Some(_) if tables.aliases.canonical(&job.mime) == super::thumbspec::IIQ_MIME => Some(job.path.clone()),
        Some(_) => rawpreview::ensure_in(raw_dir, &job.path).map(|copy| copy.file),
        None => Some(job.path.clone()),
    }
}

// A timeout on a non-local file records nothing, so a slow share never stays broken.
pub(crate) fn records_marker(ran: &Ran, class: &str) -> bool {
    match ran {
        Ran::Succeeded | Ran::Failed => true,
        Ran::TimedOut => class.is_empty(),
        Ran::NotStarted => false,
    }
}

// A decoder that ran to completion and left an empty file has judged these bytes, which is exactly what a fail marker records.
pub(crate) fn wrote_something(temp: &Path) -> bool {
    std::fs::metadata(temp).map(|m| m.len() > 0).unwrap_or(false)
}

// Every failing path drops its own temp; the one path that cannot is a worker abandoned at exit, which run.rs sweeps by pid.
pub(crate) fn discard(temp: &Path) -> Outcome {
    let _ = std::fs::remove_file(temp);
    Outcome::Failed
}

pub(crate) fn record_failure(cache: &Cache, uri: &str, mtime: i64, table: &str) {
    record_failure_for(cache, uri, mtime, table, None);
}

pub(crate) fn record_failure_for(cache: &Cache, uri: &str, mtime: i64, table: &str, identity: Option<&str>) {
    let path = cache.fail_path_for(uri, identity);
    if crate::cachewrite::require(&path).is_err() { return; }
    let dir = match path.parent() {
        Some(d) => d.to_path_buf(),
        None => return,
    };
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    if let Some(temp) = exclusive_temp(&dir) {
        if write_marker(&temp, uri, mtime, Some(table)).is_err() || std::fs::rename(&temp, &path).is_err() {
            let _ = std::fs::remove_file(&temp);
        }
    }
}

#[cfg(test)]
#[path = "thumbs_tests.rs"]
mod tests;
