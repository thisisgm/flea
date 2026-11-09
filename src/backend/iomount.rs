// Path syscalls leave the loop for mount-keyed workers; see AGENTS.md "Mount workers".
use crate::error::FleaError;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
// One deadline for a single path call, well past a cold listing and under every rail bound.
pub(crate) const CALL_DEADLINE: Duration = Duration::from_secs(5);
// One deadline for a bulk pass, matching gio and mount rather than a single stat.
pub(crate) const BULK_DEADLINE: Duration = Duration::from_secs(15);
// A stuck mount answers at once until this passes, then the next request probes it again.
pub(super) const STUCK_TTL: Duration = Duration::from_secs(30);
fn stuck_table() -> &'static Mutex<HashMap<PathBuf, Instant>> {
    static STUCK: OnceLock<Mutex<HashMap<PathBuf, Instant>>> = OnceLock::new();
    STUCK.get_or_init(|| Mutex::new(HashMap::new()))
}
// A mount marked stuck inside its TTL answers at once without a worker.
pub(super) fn is_stuck(mount: &Path, ttl: Duration) -> bool {
    #[cfg(test)]
    if TEST_STUCK.with(|slot| slot.borrow().as_deref() == Some(mount)) {
        return true;
    }
    stuck_table().lock().unwrap_or_else(|poisoned| poisoned.into_inner()).get(mount).is_some_and(|marked| marked.elapsed() < ttl)
}
// A call past its deadline marks its mount, so later requests on it answer at once.
pub(super) fn mark_stuck(mount: &Path) {
    stuck_table().lock().unwrap_or_else(|poisoned| poisoned.into_inner()).insert(mount.to_path_buf(), Instant::now());
}
// A call that answered clears its mount, so the next request probes it again.
fn clear_stuck(mount: &Path) {
    stuck_table().lock().unwrap_or_else(|poisoned| poisoned.into_inner()).remove(mount);
}
// The mount a path sits under, lexical and never a stat, so a dead server costs no syscall here.
// Sample body: "1 0 8:1 / / rw - ext4 /dev/a rw" then "30 1 0:45 / /hung rw - nfs n:/s rw".
pub fn mount_key(path: &Path, body: &str) -> PathBuf {
    if let Some(root) = super::extclass::gvfs_root(path) {
        return root;
    }
    super::mountinfo::mount_entry_in(path, body).map(|e| e.mount).unwrap_or_else(|| PathBuf::from("/"))
}
// The same lookup without the gvfs root, so an unmounted path keys itself; mount_key keeps the root.
pub fn mount_key_plain(path: &Path, body: &str) -> PathBuf {
    super::mountinfo::mount_entry_in(path, body).map(|e| e.mount).unwrap_or_else(|| path.to_path_buf())
}
// A mount whose syscalls can wedge: gvfs, network fstypes and any FUSE daemon.
pub fn is_remote(path: &Path, body: &str) -> bool {
    if super::extclass::gvfs_class(path).is_some() {
        return true;
    }
    super::mountinfo::mount_entry_in(path, body).is_some_and(|e| {
        super::netfs::can_wedge(&e.fstype)
    })
}
// One plain sentence naming the mount: "<mount> is not responding."
fn not_responding(where_: &str, path: &Path, mount: &Path) -> FleaError {
    FleaError {
        where_: where_.to_string(),
        path: path.to_string_lossy().to_string(),
        msg: format!("{} is not responding.", mount.display()),
    }
}
// Single-path bound: CALL_DEADLINE on a worker, then stuck for STUCK_TTL.
pub fn call<T: Send + 'static>(
    path: &Path,
    body: &str,
    where_: &str,
    f: impl FnOnce() -> T + Send + 'static,
) -> Result<T, FleaError> {
    call_with(path, body, where_, CALL_DEADLINE, STUCK_TTL, f)
}
// A worker that died before answering is this machine's fault, never the mount's.
pub(crate) fn internal_failure(where_: &str, path: &Path) -> FleaError {
    FleaError {
        where_: where_.to_string(),
        path: path.to_string_lossy().to_string(),
        msg: "the operation stopped without answering.".to_string(),
    }
}
// A remote call runs on a worker with a deadline; a local call runs inline with no hop.
pub(super) fn call_with<T: Send + 'static>(
    path: &Path,
    body: &str,
    where_: &str,
    deadline: Duration,
    ttl: Duration,
    f: impl FnOnce() -> T + Send + 'static,
) -> Result<T, FleaError> {
    call_with_flag(path, body, where_, deadline, ttl, |_: T| {}, f)
}
// One delivery decided once: the worker claims DELIVERED before its send, the loop claims ABANDONED on its timeout.
const PENDING: u8 = 0;
const DELIVERED: u8 = 1;
const ABANDONED: u8 = 2;
// The shared state both sides race on, so a value delivered beside a timeout is taken and never dropped.
struct Delivery {
    state: std::sync::atomic::AtomicU8,
}
impl Delivery {
    fn new() -> Delivery {
        Delivery { state: std::sync::atomic::AtomicU8::new(PENDING) }
    }
    // Worker side, run before the send: true means send, false means the loop gave up and the value goes back.
    fn worker_claim(&self) -> bool {
        self.state.compare_exchange(PENDING, DELIVERED, std::sync::atomic::Ordering::SeqCst, std::sync::atomic::Ordering::SeqCst).is_ok()
    }
    // Loop side, run on a timeout: true means abandoned, false means the worker delivered and the value is taken.
    fn loop_abandon(&self) -> bool {
        self.state.compare_exchange(PENDING, ABANDONED, std::sync::atomic::Ordering::SeqCst, std::sync::atomic::Ordering::SeqCst).is_ok()
    }
}
// A late value goes back here instead of the floor; the list worker hands its watch to the loop through it.
fn call_with_flag<T: Send + 'static>(
    path: &Path,
    body: &str,
    where_: &str,
    deadline: Duration,
    ttl: Duration,
    late: impl FnOnce(T) + Send + 'static,
    f: impl FnOnce() -> T + Send + 'static,
) -> Result<T, FleaError> {
    #[cfg(test)]
    CALLS.with(|calls| calls.set(calls.get() + 1));
    let mount = mount_key(path, body);
    if is_stuck(&mount, ttl) {
        return Err(not_responding(where_, path, &mount));
    }
    if !is_remote(path, body) {
        return Ok(f());
    }
    let decided = std::sync::Arc::new(Delivery::new());
    let worker = std::sync::Arc::clone(&decided);
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let value = f();
        if worker.worker_claim() {
            let _ = tx.send(value);
        } else {
            late(value);
        }
    });
    match rx.recv_timeout(deadline) {
        Ok(value) => {
            clear_stuck(&mount);
            Ok(value)
        }
        // A panic drops tx, so only a deadline still running marks the mount stuck; a delivery racing it is taken.
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            if decided.loop_abandon() {
                mark_stuck(&mount);
                Err(not_responding(where_, path, &mount))
            } else if let Ok(value) = rx.recv() {
                clear_stuck(&mount);
                Ok(value)
            } else {
                Err(internal_failure(where_, path))
            }
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(internal_failure(where_, path)),
    }
}
// The mount table every write decision reads; tests map a sandbox onto a fake mount through it.
pub fn mount_body() -> String {
    #[cfg(test)]
    if let Some(body) = TEST_BODY.with(|slot| slot.borrow().clone()) {
        return body;
    }
    std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default()
}
// A remote write either answers in time or stays running past the deadline; the loop moves on and the worker reports late.
pub enum SlowWrite<T> {
    Ready(Result<T, FleaError>),
    Slow { mount: PathBuf, rx: std::sync::mpsc::Receiver<Result<T, FleaError>> },
}
// One whole sentence per operation, each with {mount} for the slow mount.
pub const SLOW_RENAME: &str = "{mount} is slow. The rename will finish on its own.";
pub const SLOW_MKDIR: &str = "{mount} is slow. The new folder will finish on its own.";
pub const SLOW_BULK_RENAME: &str = "{mount} is slow. The bulk rename will finish on its own.";
pub const SLOW_TAGS: &str = "{mount} is slow. The tag change will finish on its own.";
// The operation's sentence with its mount filled in.
pub fn slow_sentence(mount: &Path, sentence: &str) -> String {
    sentence.replace("{mount}", &mount.display().to_string())
}
// A remote write runs on its own worker with the deadline, a local write runs inline with no hop; the deadline is a parameter so tests hold a worker past a short bound instead of the production five seconds.
pub fn slow_write_with<T: Send + 'static>(
    path: &Path,
    body: &str,
    where_: &str,
    deadline: Duration,
    f: impl FnOnce() -> Result<T, FleaError> + Send + 'static,
) -> SlowWrite<T> {
    #[cfg(test)]
    CALLS.with(|calls| calls.set(calls.get() + 1));
    let mount = mount_key(path, body);
    if !is_remote(path, body) {
        return SlowWrite::Ready(f());
    }
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    match rx.recv_timeout(deadline) {
        Ok(done) => SlowWrite::Ready(done),
        // A write still running is never a failure and never marks its mount: its worker reports late.
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => SlowWrite::Slow { mount, rx },
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => SlowWrite::Ready(Err(internal_failure(where_, path))),
    }
}
// Thread-local, so parallel suites never see each other's calls; tests reset it with test_reset.
#[cfg(test)]
thread_local! {
    static CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
// One thread's refusal fixture is isolated from parallel tests that clear the production stuck table.
#[cfg(test)]
thread_local! {
    static TEST_STUCK: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}
// The guard restores the previous fixture even when a wire assertion panics.
#[cfg(test)]
pub(crate) struct StuckGuard(Option<PathBuf>);
// Holds the real bound on its stuck-mount error path without a clock or remote filesystem.
#[cfg(test)]
pub(crate) fn test_hold_stuck(mount: PathBuf) -> StuckGuard {
    StuckGuard(TEST_STUCK.with(|slot| slot.replace(Some(mount))))
}
#[cfg(test)]
impl Drop for StuckGuard {
    fn drop(&mut self) {
        TEST_STUCK.with(|slot| *slot.borrow_mut() = self.0.take());
    }
}
// Thread-local, so one suite mapping its sandbox onto a fake mount never leaks into the next.
#[cfg(test)]
thread_local! {
    static TEST_BODY: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}
// Holds mount_body on the fake table until the guard drops, so a sandbox reads as remote.
#[cfg(test)]
pub struct BodyGuard;
// Maps mount_body onto the fake table; the worker never reads it, so holding it is race free.
#[cfg(test)]
pub fn test_hold_body(body: String) -> BodyGuard {
    TEST_BODY.with(|slot| *slot.borrow_mut() = Some(body));
    BodyGuard
}
// Dropping the guard hands mount_body back to the live table, so no later test reads the fake one.
#[cfg(test)]
impl Drop for BodyGuard {
    fn drop(&mut self) {
        TEST_BODY.with(|slot| *slot.borrow_mut() = None);
    }
}
// How many bounded calls this thread made since the last reset, local or remote alike.
#[cfg(test)]
pub fn test_calls() -> usize {
    CALLS.with(|calls| calls.get())
}
// Bulk passes share the single-call sentence and bound, with their own longer deadline.
pub fn call_bulk<T: Send + 'static>(
    path: &Path,
    body: &str,
    where_: &str,
    f: impl FnOnce() -> T + Send + 'static,
) -> Result<T, FleaError> {
    call_with(path, body, where_, BULK_DEADLINE, STUCK_TTL, f)
}
// Resets this thread's call count; the stuck table is shared by parallel tests, each keying its marks by its own mount, so a clear would land inside another test.
#[cfg(test)]
pub fn test_reset() {
    CALLS.with(|calls| calls.set(0));
}
// One listing's syscalls, computed on a worker with the listing, never on the loop.
pub struct ListOut {
    pub listing: super::listing::Listing,
    pub read_ms: f64,
    pub sort_ms: f64,
    pub sized: Vec<Option<super::dirsize::DirSize>>,
    pub dev: u64,
    pub writable: bool,
    pub first_metas: Vec<super::meta::Meta>,
    pub first_start: usize,
    pub first_ms: f64,
    pub watch_wd: i32,
    pub watch_mount: std::path::PathBuf,
}
// A failed listing carries its mode for the denial tile, or zero when no stat ran, with the armed descriptor or -1 when none needs removing.
#[derive(Debug)]
pub struct ListErr {
    pub error: FleaError,
    pub mode: u32,
    pub watch_wd: i32,
}
// The loop gave up first, so every descriptor the scan armed, expanded children included, goes back for its guarded drop.
fn hand_back_late(out: &Result<ListOut, ListErr>, tx: &super::events::Sender<super::events::Event>) {
    if let Ok(done) = out {
        for &wd in &done.listing.outline_watches {
            let _ = tx.send(super::events::Event::AbandonWatch(wd));
        }
    }
    let wd = match out { Ok(done) => done.watch_wd, Err(failed) => failed.watch_wd };
    if wd >= 0 {
        let _ = tx.send(super::events::Event::AbandonWatch(wd));
    }
}
// Sample request: {"c":"list","path":"/hung/dir","first":350,"hidden":false} on an nfs mount.
// Eight inputs: the request fields plus the mount table they classify against.
#[allow(clippy::too_many_arguments)]
pub fn list_dir(
    path: String,
    hidden: bool,
    first: usize,
    line: String,
    mime: std::sync::Arc<super::mime::Db>,
    fd: std::ffi::c_int,
    loop_tx: super::events::Sender<super::events::Event>,
    body: &str,
) -> Result<ListOut, ListErr> {
    let path_buf = flea::bytename::exact(&path);
    let late_tx = loop_tx.clone();
    call_with_flag(&path_buf, body, "scan", CALL_DEADLINE, STUCK_TTL, move |out: Result<ListOut, ListErr>| hand_back_late(&out, &late_tx), move || {
        let base = flea::bytename::exact(&path);
        let wd = super::watch::Watch::add_raw(fd, &base);
        match super::scan::scan(&path, hidden) {
            Ok((mut l, read_ms)) => {
                super::picker::filter_listing(&mut l, &mime, &line);
                let (pass_ms, sort_ms, mut sized) = match super::ordering::request(&mut l, &base, &mime, &line) {
                    Ok(timing) => timing,
                    Err(msg) => {
                        let error = FleaError { where_: "sort".to_string(), path: path.clone(), msg: msg.to_string() };
                        return Err(ListErr { error, mode: 0, watch_wd: wd });
                    }
                };
                let (child_read, child_sort) = super::outline::expand(&mut l, &base, hidden, &line, mime.clone(), fd, loop_tx.clone());
                // Child rows shift every row after them, so sizes keyed by row index would land on the wrong rows.
                if !l.outline_open.is_empty() { sized.clear(); }
                let dev = super::fsinfo::dev_of(&base);
                let writable = super::ops::dir_writable(&base);
                let first_start = l.first_start(&base, crate::json::field_str(&line, "select").as_deref(), first);
                let (first_metas, first_ms) = super::meta::stat_range(&base, &l, first_start, first);
                Ok(ListOut { listing: l, read_ms: read_ms + pass_ms + child_read, sort_ms: sort_ms + child_sort, sized, dev, writable, first_metas, first_start, first_ms, watch_wd: wd, watch_mount: super::watch::live_mount_of(&base) })
            }
            Err(e) => {
                let mode = super::scan::mode_of(&path);
                Err(ListErr { error: e, mode, watch_wd: wd })
            }
        }
    })
    .map_err(|timeout| ListErr { error: timeout, mode: 0, watch_wd: -1 })
    .and_then(|inner| inner)
}
#[cfg(test)]
#[path = "iomount_tests.rs"]
mod tests;
