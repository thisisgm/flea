use crate::backend::aliases::Aliases;
use crate::backend::child::Ran;
use crate::backend::sandbox;
use crate::backend::sandbox_broker::Client;
use crate::backend::sandbox_exec::Launch;
use crate::backend::thumbargv::argv;
use crate::backend::thumbcache::{uri_for, Cache};
use crate::backend::thumbspec::Thumbnailers;
use crate::backend::thumbwrite::{exclusive_temp, stamp, write_marker};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

pub const MAX_QUEUE: usize = 70;
pub const THUMB_SIZE: u32 = 256;
const JOB_TIMEOUT: Duration = Duration::from_secs(20);
pub struct Job {
    pub path: PathBuf,
    pub mtime: i64,
    pub mime: String,
    // None unless FLEA_THUMB_TRACE is set, so an untraced job carries no marks; see AGENTS.md "Thumbnail trace".
    pub trace: Option<Trace>,
}
pub struct Trace {
    pub row: usize,
    pub depth: usize,
    pub at: Instant,
    pub popped: Duration,
    pub spawned: Duration,
    pub exited: Duration,
}

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
    pub path: PathBuf,
    pub result: Outcome,
    pub ms: f64,
    pub trace: Option<Trace>,
}

struct Tables {
    aliases: Arc<Aliases>,
    specs: Arc<Thumbnailers>,
    cache: Cache,
}

type Shared = Arc<(Mutex<VecDeque<Job>>, Condvar)>;

pub struct Pool {
    inner: Shared,
}

impl Pool {
    // The cache root and both tables are the caller's: a test never writes into the operator's shared cache, and run.rs has already parsed these two files.
    pub fn new(workers: usize, results: Sender<Done>, root: PathBuf, aliases: Arc<Aliases>, specs: Arc<Thumbnailers>) -> Pool {
        Pool::start(workers, results, Tables { aliases, specs, cache: Cache::at(root) })
    }

    #[cfg(test)]
    fn with_specs(workers: usize, results: Sender<Done>, root: PathBuf, entries: &[(String, String)]) -> Pool {
        let aliases = Arc::new(Aliases::load());
        let specs = Arc::new(Thumbnailers::from_entries(entries, &aliases));
        Pool::start(workers, results, Tables { aliases, specs, cache: Cache::at(root) })
    }

    fn start(workers: usize, results: Sender<Done>, tables: Tables) -> Pool {
        let inner: Shared = Arc::new((Mutex::new(VecDeque::new()), Condvar::new()));
        let tables = Arc::new(tables);
        for _ in 0..workers.max(1) {
            let inner = Arc::clone(&inner);
            let tables = Arc::clone(&tables);
            let results = results.clone();
            std::thread::spawn(move || worker(inner, results, tables));
        }
        Pool { inner }
    }

    // Returns the jobs it dropped to make room, so a caller can unmap and answer the rows that will now never report.
    pub fn submit(&self, mut job: Job) -> Vec<Job> {
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
    pub fn cancel(&self, path: &Path) -> Vec<Job> {
        let (lock, _cv) = &*self.inner;
        let mut q = lock.lock().unwrap();
        let (dropped, kept): (Vec<Job>, Vec<Job>) = q.drain(..).partition(|j| j.path == path);
        *q = kept.into();
        dropped
    }

    pub fn cancel_all(&self) -> Vec<Job> {
        let (lock, _cv) = &*self.inner;
        let mut q = lock.lock().unwrap();
        q.drain(..).collect()
    }

    #[cfg(test)]
    fn pending(&self) -> usize {
        let (lock, _cv) = &*self.inner;
        lock.lock().unwrap().len()
    }
}

fn worker(inner: Shared, results: Sender<Done>, tables: Arc<Tables>) {
    let mut broker = Client::new();
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
        let outcome = run_one(&tables, &mut broker, &mut job);
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        if results.send(Done { path: job.path, result: outcome, ms, trace: job.trace }).is_err() {
            return;
        }
    }
}

fn run_one(tables: &Tables, broker: &mut Client, job: &mut Job) -> Outcome {
    let spec = match tables.specs.for_mime(&job.mime, &tables.aliases) {
        Some(s) => s,
        None => return Outcome::Failed,
    };
    // The cache key is the URI of the path the user named, because that is the key every other application on the box looks under; see AGENTS.md "Thumbnail cache".
    let key_uri = uri_for(&job.path);
    let final_path = tables.cache.large_path(&key_uri);
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
    let (abs, inner) = match argv(spec, &job.path, &temp, THUMB_SIZE) {
        Some(a) => a,
        None => return discard(&temp),
    };
    // corner: a missing sandbox program fails closed and records nothing, because a missing package is not a broken file; see AGENTS.md "Thumbnail sandbox".
    if !sandbox::available() {
        return discard(&temp);
    }
    // corner: a thumbnailer that wrote then renamed would fail against this file bind, and only glycin and ffmpegthumbnailer were probed; see AGENTS.md "Thumbnail pool".
    let launch = match Launch::new(&inner, &abs, &temp) {
        Ok(launch) => launch,
        Err(_) => return discard(&temp),
    };
    if let Some(t) = job.trace.as_mut() {
        t.spawned = t.at.elapsed();
    }
    let ran = broker.run_checked(&launch.argv, JOB_TIMEOUT, &launch.error);
    if let Some(t) = job.trace.as_mut() {
        t.exited = t.at.elapsed();
    }
    let wrote = wrote_something(&temp);
    if !(ran == Ran::Succeeded && wrote) {
        if records_failure(ran, wrote) {
            // glycin exits zero on bad bytes. An empty output is still the decoder's verdict.
            record_failure(&tables.cache, &key_uri, job.mtime);
        }
        // A child that never started is a machine failure, so it does not mark the file.
        return discard(&temp);
    }
    // The child wrote a bare PNG, so the spec's own metadata is added before the file is published.
    if stamp(&temp, &key_uri, job.mtime).is_err() || std::fs::rename(&temp, &final_path).is_err() {
        return discard(&temp);
    }
    Outcome::Ready(final_path)
}

fn wrote_something(temp: &Path) -> bool {
    std::fs::metadata(temp).map(|m| m.len() > 0).unwrap_or(false)
}

fn records_failure(ran: Ran, wrote: bool) -> bool {
    !matches!((ran, wrote), (Ran::Succeeded, true) | (Ran::NotStarted, _))
}

// Every failing path drops its own temp; the one path that cannot is a worker abandoned at exit, which run.rs sweeps by pid.
fn discard(temp: &Path) -> Outcome {
    let _ = std::fs::remove_file(temp);
    Outcome::Failed
}

fn record_failure(cache: &Cache, uri: &str, mtime: i64) {
    let path = cache.fail_path(uri);
    let dir = match path.parent() {
        Some(d) => d.to_path_buf(),
        None => return,
    };
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    if let Some(temp) = exclusive_temp(&dir) {
        if write_marker(&temp, uri, mtime).is_err() || std::fs::rename(&temp, &path).is_err() {
            let _ = std::fs::remove_file(&temp);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::thumbcache::png_text;
    use std::sync::mpsc::{channel, Receiver};

    const MISSING: &str = "/definitely/not/here.jpg";
    const FIXTURE_MTIME: i64 = 1787790423;

    // Every test roots its cache under its own directory, so the operator's shared cache is never written.
    fn root(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("flea-thumbs-{}-{}", tag, std::process::id()))
    }

    fn job(path: &str) -> Job {
        Job { path: PathBuf::from(path), mtime: 0, mime: "image/jpeg".to_string(), trace: None }
    }

    // Returns a pool whose one worker is already inside a ten minute child, so the queue can only change by the caller's own hand.
    fn pinned(tag: &str, base: u32) -> (PathBuf, Pool, Receiver<Done>) {
        let (tx, rx) = channel();
        let dir = root(tag);
        // A duration nothing else on the box shares, so the gate below cannot be satisfied by another test's child or another suite's.
        let seconds = format!("{}.{}", base, std::process::id());
        let body = format!("[Thumbnailer Entry]\nTryExec=/usr/bin/sleep\nExec=/usr/bin/sleep {}\nMimeType=image/jpeg;\n", seconds);
        let entries = [("pin.thumbnailer".to_string(), body)];
        let pool = Pool::with_specs(1, tx, dir.clone(), &entries);
        // The pin's input only has to canonicalise, and it sits outside the cache root so the teardown cannot race the worker.
        pool.submit(job("/usr/bin/sleep"));
        for _ in 0..400 {
            if pin_running(&seconds) {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        // The child has exec'd, which it can only do after bwrap made every bind, so removing the temp afterwards cannot fail the job.
        assert!(pin_running(&seconds), "the pin child never started");
        assert_eq!(pool.pending(), 0);
        (dir, pool, rx)
    }

    // Sample input, /proc/<pid>/cmdline for the pin: the program and its one argument, NUL terminated, which is an exact match and not a prefix of bwrap's own argv.
    fn pin_running(seconds: &str) -> bool {
        let want = format!("/usr/bin/sleep\0{}\0", seconds);
        let procs = match std::fs::read_dir("/proc") {
            Ok(p) => p,
            Err(_) => return false,
        };
        procs.flatten().any(|p| std::fs::read(p.path().join("cmdline")).map(|c| c == want.as_bytes()).unwrap_or(false))
    }

    #[test]
    fn a_cancelled_job_never_runs() {
        let (dir, pool, rx) = pinned("cancel", 601);
        pool.submit(job(MISSING));
        assert_eq!(pool.pending(), 1);
        pool.cancel(&PathBuf::from(MISSING));
        assert_eq!(pool.pending(), 0, "cancel left the job in the queue");
        // A cancelled job produces no Done at all, so a short wait must time out.
        let seen = rx.recv_timeout(Duration::from_millis(500));
        std::fs::remove_dir_all(&dir).ok();
        assert!(seen.is_err(), "a job reported when none should have");
    }

    #[test]
    fn cancel_all_empties_the_queue() {
        let (dir, pool, _rx) = pinned("cancelall", 602);
        for i in 0..8 {
            pool.submit(job(&format!("/definitely/not/here-{}.jpg", i)));
        }
        assert_eq!(pool.pending(), 8);
        pool.cancel_all();
        let left = pool.pending();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(left, 0);
    }

    #[test]
    fn the_queue_is_bounded_and_drops_the_oldest_rather_than_growing() {
        let (dir, pool, _rx) = pinned("bounded", 603);
        for i in 0..(MAX_QUEUE * 2) {
            pool.submit(job(&format!("/definitely/not/here-{}.jpg", i)));
        }
        assert_eq!(pool.pending(), MAX_QUEUE);
        // The oldest submissions are the dropped ones, so cancelling one of those changes nothing.
        pool.cancel(&PathBuf::from("/definitely/not/here-0.jpg"));
        assert_eq!(pool.pending(), MAX_QUEUE);
        pool.cancel(&PathBuf::from(format!("/definitely/not/here-{}.jpg", MAX_QUEUE * 2 - 1)));
        let left = pool.pending();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(left, MAX_QUEUE - 1);
    }

    #[test]
    fn a_missing_input_reports_failed_rather_than_hanging() {
        let (tx, rx) = channel();
        let dir = root("missing");
        let aliases = Arc::new(Aliases::load());
        let pool = Pool::new(1, tx, dir.clone(), Arc::clone(&aliases), Arc::new(Thumbnailers::load(&aliases)));
        pool.submit(job(MISSING));
        let done = rx.recv_timeout(Duration::from_secs(10)).expect("no result");
        assert_eq!(done.path, PathBuf::from(MISSING));
        assert!(matches!(done.result, Outcome::Failed));
        // A vanished input is not a broken file, so nothing is recorded and no temp is left behind.
        let recorded = dir.join("fail").exists();
        let left = std::fs::read_dir(dir.join("large")).unwrap().count();
        std::fs::remove_dir_all(&dir).ok();
        assert!(!recorded, "a vanished input was recorded in fail/");
        assert_eq!(left, 0, "a temp file survived a failed job");
    }

    #[test]
    fn a_real_file_round_trips_to_a_stamped_cache_entry() {
        let (tx, rx) = channel();
        let dir = root("roundtrip");
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("in.png");
        // A real thumbnailer needs a real image, and the fail marker writer already makes the smallest valid one.
        write_marker(&src, "file:///input", 0).unwrap();
        let aliases = Arc::new(Aliases::load());
        let pool = Pool::new(1, tx, dir.clone(), Arc::clone(&aliases), Arc::new(Thumbnailers::load(&aliases)));
        pool.submit(Job { path: src.clone(), mtime: FIXTURE_MTIME, mime: "image/png".to_string(), trace: None });
        let done = rx.recv_timeout(Duration::from_secs(30)).expect("no result");
        let out = match done.result {
            Outcome::Ready(p) => p,
            Outcome::Failed => panic!("a real thumbnailer failed on a valid png"),
        };
        let bytes = std::fs::read(&out).unwrap();
        let published = std::fs::read_dir(dir.join("large")).unwrap().count();
        let want = Cache::at(dir.clone()).large_path(&uri_for(&src));
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(out, want);
        assert_eq!(png_text(&bytes, "Thumb::URI"), Some(uri_for(&src)));
        assert_eq!(png_text(&bytes, "Thumb::MTime"), Some(FIXTURE_MTIME.to_string()));
        assert_eq!(published, 1, "a temp file survived the publish");
    }

    #[test]
    fn only_decoder_verdicts_and_deadlines_record_failure_markers() {
        assert!(records_failure(Ran::Succeeded, false), "an empty successful decode is a decoder verdict");
        assert!(records_failure(Ran::Failed, false), "a failure or deadline is a decoder verdict");
        assert!(!records_failure(Ran::Succeeded, true));
        assert!(!records_failure(Ran::NotStarted, false), "infrastructure failure must not mark the file");
    }
}
