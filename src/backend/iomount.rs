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
    loop_tx: std::sync::mpsc::Sender<super::events::Event>,
    body: &str,
) -> Result<ListOut, ListErr> {
    let path_buf = std::path::PathBuf::from(&path);
    call_with_flag(&path_buf, body, "scan", CALL_DEADLINE, STUCK_TTL, move |out: Result<ListOut, ListErr>| {
        // The loop gave up first, so the late descriptor goes back for its guarded drop on the loop.
        let wd = match &out { Ok(done) => done.watch_wd, Err(failed) => failed.watch_wd };
        if wd >= 0 {
            let _ = loop_tx.send(super::events::Event::AbandonWatch(wd));
        }
    }, move || {
        let base = std::path::PathBuf::from(&path);
        let wd = super::watch::Watch::add_raw(fd, &base);
        match super::scan::scan(&path, hidden) {
            Ok((mut l, read_ms)) => {
                super::picker::filter_listing(&mut l, &mime, &line);
                let (pass_ms, sort_ms, sized) = match super::ordering::request(&mut l, &base, &mime, &line) {
                    Ok(timing) => timing,
                    Err(msg) => {
                        let error = FleaError { where_: "sort".to_string(), path: path.clone(), msg: msg.to_string() };
                        return Err(ListErr { error, mode: 0, watch_wd: wd });
                    }
                };
                let dev = super::fsinfo::dev_of(&base);
                let writable = super::ops::dir_writable(&base);
                let first_start = l.first_start(&base, crate::json::field_str(&line, "select").as_deref(), first);
                let (first_metas, first_ms) = super::meta::stat_range(&base, &l, first_start, first);
                Ok(ListOut { listing: l, read_ms: read_ms + pass_ms, sort_ms, sized, dev, writable, first_metas, first_start, first_ms, watch_wd: wd, watch_mount: super::watch::live_mount_of(&base) })
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
mod tests {
    use super::*;
    use std::sync::mpsc::channel;
    // Fast enough to keep the suite moving, slow enough that scheduling noise never flakes it.
    const TEST_DEADLINE: Duration = Duration::from_millis(300);
    // Long enough that a stuck mark outlives the whole test, so the fast path is pinned.
    const TEST_TTL: Duration = Duration::from_secs(60);
    // Outer bound for the thread running call, past one deadline with room for spawn.
    const TEST_ASSERT_BOUND: Duration = Duration::from_secs(2);
    // Margin past a slow deadline, so scheduling noise never flakes the slow proof.
    const TEST_SLOW_MARGIN: Duration = Duration::from_secs(2);
    // A stuck mark answers at once, far inside any deadline, so this pins the fast path.
    const TEST_FAST_BOUND: Duration = Duration::from_millis(100);
    // One test's own pair of nfs mounts, named for it, so parallel tests never share a stuck mark through a mount name.
    // Sample body: "1 0 8:1 / / rw - ext4 /dev/a rw\n30 1 0:45 / /hung-<tag> rw - nfs n:/s rw\n31 1 0:46 / /hung2-<tag> rw - nfs n:/t rw\n".
    struct Mounts {
        body: String,
        hung: PathBuf,
        neighbour: PathBuf,
    }
    impl Mounts {
        fn new(tag: &str) -> Mounts {
            let hung = PathBuf::from(format!("/hung-{tag}"));
            let neighbour = PathBuf::from(format!("/hung2-{tag}"));
            let body = format!("1 0 8:1 / / rw - ext4 /dev/a rw\n30 1 0:45 / {} rw - nfs n:/s rw\n31 1 0:46 / {} rw - nfs n:/t rw\n",
                hung.display(), neighbour.display());
            Mounts { body, hung, neighbour }
        }
        fn hung_dir(&self) -> PathBuf {
            self.hung.join("dir")
        }
        fn neighbour_dir(&self) -> PathBuf {
            self.neighbour.join("dir")
        }
    }
    // Parallel tests share the stuck table, so a reset in one must never erase a mark another is relying on.
    #[test]
    fn a_reset_leaves_another_tests_stuck_mark_alone() {
        let mount = PathBuf::from("/hung-resetkeeps");
        mark_stuck(&mount);
        test_reset();
        assert!(is_stuck(&mount, TEST_TTL), "a reset never clears a mark it did not make");
        clear_stuck(&mount);
    }
    #[test]
    fn a_hung_mount_answers_not_responding_while_a_healthy_mount_still_answers() {
        let m = Mounts::new("hungmount");
        assert!(is_remote(&m.neighbour_dir(), &m.body), "the neighbour is a second remote mount with its own worker");
        let (tx_hung, rx_hung) = channel::<()>();
        let hung_done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let hung_flag = std::sync::Arc::clone(&hung_done);
        let hung_path = m.hung_dir();
        let hung_body = m.body.clone();
        let (tx_res, rx_res) = channel();
        std::thread::spawn(move || {
            let out = call_with(
                &hung_path,
                &hung_body,
                "scan",
                TEST_DEADLINE,
                TEST_TTL,
                move || {
                    let _ = rx_hung.recv();
                    hung_flag.store(true, std::sync::atomic::Ordering::SeqCst);
                    42
                },
            );
            let _ = tx_res.send(out);
        });
        let hung = rx_res.recv_timeout(TEST_ASSERT_BOUND).expect("a hung mount must answer within its bound");
        let err = hung.expect_err("a hung mount answers with an error, never a value");
        assert!(err.msg.contains("not responding"), "the sentence names the cause: {}", err.msg);
        assert_eq!(err.where_, "scan");
        let neighbour_ran = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let neighbour_flag = std::sync::Arc::clone(&neighbour_ran);
        let healthy = call_with(&m.neighbour_dir(), &m.body, "scan", TEST_DEADLINE, TEST_TTL, move || {
            neighbour_flag.store(true, std::sync::atomic::Ordering::SeqCst);
            7
        });
        assert_eq!(healthy.unwrap(), 7, "a stuck mount never blocks its neighbour");
        assert!(neighbour_ran.load(std::sync::atomic::Ordering::SeqCst), "the neighbour ran on its own worker");
        // A second call on the stuck mount answers at once without running its closure.
        let stuck_ran = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stuck_flag = std::sync::Arc::clone(&stuck_ran);
        let t = std::time::Instant::now();
        let again = call_with(&m.hung_dir(), &m.body, "scan", TEST_DEADLINE, TEST_TTL, move || {
            stuck_flag.store(true, std::sync::atomic::Ordering::SeqCst);
            42
        });
        let fast = t.elapsed();
        let err = again.expect_err("a stuck mount answers with an error, never a value");
        assert!(err.msg.contains("not responding"), "the sentence names the cause: {}", err.msg);
        assert!(!stuck_ran.load(std::sync::atomic::Ordering::SeqCst), "the stuck fast path runs no worker");
        assert!(fast < TEST_FAST_BOUND, "the stuck fast path answers at once: {:?}", fast);
        drop(tx_hung);
    }
    #[test]
    fn every_slow_operation_has_its_own_whole_sentence() {
        let mount = Path::new("/hung");
        let said: Vec<String> = [SLOW_RENAME, SLOW_MKDIR, SLOW_BULK_RENAME, SLOW_TAGS].iter().map(|s| slow_sentence(mount, s)).collect();
        assert_eq!(said, [
            "/hung is slow. The rename will finish on its own.",
            "/hung is slow. The new folder will finish on its own.",
            "/hung is slow. The bulk rename will finish on its own.",
            "/hung is slow. The tag change will finish on its own.",
        ]);
    }
    #[test]
    fn a_worker_that_dies_marks_nothing_stuck_and_says_so() {
        let m = Mounts::new("deadworker");
        // The closure panics, so tx drops and recv_timeout answers Disconnected.
        crate::backend::quietpanic::quiet();
        let dead = call_with(&m.hung_dir(), &m.body, "scan", TEST_DEADLINE, TEST_TTL, || -> i32 {
            panic!("{}", crate::backend::quietpanic::DELIBERATE);
        });
        let err = dead.expect_err("a dead worker answers with an error, never a value");
        assert!(!err.msg.contains("not responding"), "a dead worker is not a stuck mount: {}", err.msg);
        assert!(err.msg.contains("without answering"), "the sentence names the dead worker: {}", err.msg);
        let live = call_with(&m.hung_dir(), &m.body, "scan", TEST_DEADLINE, TEST_TTL, || 7);
        assert_eq!(live.unwrap(), 7, "a dead worker never marks its mount stuck");
    }
    #[test]
    fn a_slow_write_answers_slow_within_its_deadline_while_a_second_mount_answers() {
        let m = Mounts::new("slowwrite");
        let (release, wait) = channel::<()>();
        let (tx_slow, rx_slow) = channel();
        let (slow_dir, slow_body) = (m.hung_dir(), m.body.clone());
        std::thread::spawn(move || {
            let out = slow_write_with(&slow_dir, &slow_body, "rename", TEST_DEADLINE, move || {
                let _ = wait.recv();
                Ok::<i32, FleaError>(42)
            });
            let _ = tx_slow.send(out);
        });
        // Past one deadline the held write answers Slow, never a failure for work still running.
        let out = rx_slow.recv_timeout(TEST_DEADLINE + TEST_SLOW_MARGIN).expect("a held write answers Slow within its deadline");
        let (mount, pending) = match out {
            SlowWrite::Slow { mount, rx } => (mount, rx),
            SlowWrite::Ready(_) => panic!("a write still running answers Slow, never Ready"),
        };
        assert_eq!(mount, m.hung, "the slow answer names the mount, not the file");
        // While the first mount is held, a second mount answers on its own worker.
        let neighbour = slow_write_with(&m.neighbour_dir(), &m.body, "scan", TEST_DEADLINE, || Ok::<i32, FleaError>(7));
        match neighbour {
            SlowWrite::Ready(Ok(7)) => {}
            _ => panic!("a held mount never blocks its neighbour"),
        }
        drop(release);
        let landed = pending.recv_timeout(TEST_ASSERT_BOUND).expect("a released write lands");
        assert_eq!(landed.unwrap(), 42, "the held write runs to its end and reports");
        // A slow write never marks its mount stuck: a read right after still runs its closure.
        let ran = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = std::sync::Arc::clone(&ran);
        let live = call_with(&m.hung_dir(), &m.body, "scan", TEST_DEADLINE, TEST_TTL, move || {
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
            7
        });
        assert_eq!(live.unwrap(), 7, "a slow write never marks its mount stuck");
        assert!(ran.load(std::sync::atomic::Ordering::SeqCst), "the read after a slow line runs its closure");
    }
    #[test]
    fn a_write_that_answers_in_time_reports_ready_with_no_slow() {
        let m = Mounts::new("readyfast");
        let fast = slow_write_with(&m.hung_dir(), &m.body, "rename", TEST_DEADLINE, || Ok::<i32, FleaError>(7));
        match fast {
            SlowWrite::Ready(Ok(7)) => {}
            _ => panic!("a fast write answers Ready, never Slow"),
        }
        // A local mount runs inline with no hop, so its write never waits on a worker.
        let local = slow_write_with(Path::new("/elsewhere/dir"), &m.body, "rename", TEST_DEADLINE, || Ok::<i32, FleaError>(7));
        match local {
            SlowWrite::Ready(Ok(7)) => {}
            _ => panic!("a local write answers inline"),
        }
    }
    #[test]
    fn a_slow_worker_that_dies_marks_nothing_stuck_and_says_so() {
        let m = Mounts::new("slowdead");
        // The closure panics, so tx drops and the slow wait answers Disconnected.
        crate::backend::quietpanic::quiet();
        let dead = slow_write_with(&m.hung_dir(), &m.body, "rename", TEST_DEADLINE, || -> Result<i32, FleaError> {
            panic!("{}", crate::backend::quietpanic::DELIBERATE);
        });
        match dead {
            SlowWrite::Ready(Err(err)) => assert!(err.msg.contains("without answering"), "a dead worker is not a stuck mount: {}", err.msg),
            _ => panic!("a dead worker answers Ready with its failure, never Slow"),
        }
        let live = call_with(&m.hung_dir(), &m.body, "scan", TEST_DEADLINE, TEST_TTL, || 7);
        assert_eq!(live.unwrap(), 7, "a dead worker never marks its mount stuck");
    }
    // Decided once: exactly one side wins, whatever order the two compare_exchanges land in.
    #[test]
    fn delivery_is_decided_once_between_worker_and_loop() {
        let first = Delivery::new();
        assert!(first.worker_claim(), "the first claim wins");
        assert!(!first.loop_abandon(), "the second decision loses and takes the value");
        let second = Delivery::new();
        assert!(second.loop_abandon(), "the first decision wins");
        assert!(!second.worker_claim(), "a late worker hands its value back");
        for _ in 0..200 {
            let raced = std::sync::Arc::new(Delivery::new());
            let worker = std::sync::Arc::clone(&raced);
            let handle = std::thread::spawn(move || worker.worker_claim());
            let abandoned = raced.loop_abandon();
            assert_ne!(handle.join().unwrap(), abandoned, "one side wins each race");
        }
    }
    // A worker finishing after its timeout hands its value back instead of dropping it on a dead channel.
    #[test]
    fn a_timed_out_call_hands_its_late_value_back() {
        let m = Mounts::new("latevalue");
        let (release, wait) = channel::<()>();
        let (late_tx, late_rx) = channel::<i32>();
        let (tx_res, rx_res) = channel();
        std::thread::spawn(move || {
            let out = call_with_flag(&m.hung_dir(), &m.body, "scan", TEST_DEADLINE, TEST_TTL, move |value: i32| {
                let _ = late_tx.send(value);
            }, move || {
                let _ = wait.recv();
                42
            });
            let _ = tx_res.send(out);
        });
        let err = rx_res.recv_timeout(TEST_ASSERT_BOUND).expect("a held call answers within its bound").expect_err("a held call answers with an error");
        assert!(err.msg.contains("not responding"), "the sentence names the cause: {}", err.msg);
        drop(release);
        assert_eq!(late_rx.recv_timeout(TEST_ASSERT_BOUND).expect("the late value reaches its handoff"), 42, "the late value goes back, never to the floor");
    }
    #[test]
    fn a_failed_scan_hands_back_the_watch_it_armed() {
        let sandbox = crate::backend::testdir::TestDir::new("listwd");
        let file = sandbox.file("a.txt", "a");
        let (tx, _rx) = channel::<crate::backend::events::Event>();
        let watch = crate::backend::watch::Watch::start(tx.clone());
        let mime = std::sync::Arc::new(crate::backend::mime::Db::load());
        let failed = match list_dir(file.to_string_lossy().to_string(), false, 10, String::new(), mime, watch.raw_fd(), tx, &mount_body()) {
            Err(failed) => failed,
            Ok(_) => panic!("a file is not a listing"),
        };
        assert!(failed.watch_wd >= 0, "the loop must remove what the worker armed");
    }
    #[test]
    fn a_list_dir_carries_the_mount_its_worker_computed() {
        test_reset();
        let sandbox = crate::backend::testdir::TestDir::new("listmnt");
        let (tx, _rx) = channel::<crate::backend::events::Event>();
        let watch = crate::backend::watch::Watch::start(tx.clone());
        let mime = std::sync::Arc::new(crate::backend::mime::Db::load());
        let line = format!(r#"{{"c":"list","path":"{}","first":10}}"#, sandbox.path().to_string_lossy());
        let done = match list_dir(sandbox.path().to_string_lossy().to_string(), false, 10, line, mime, watch.raw_fd(), tx, &mount_body()) {
            Ok(done) => done,
            Err(failed) => panic!("a directory lists: {}", failed.error.msg),
        };
        assert_eq!(done.watch_mount, crate::backend::watch::live_mount_of(sandbox.path()));
        assert!(!done.watch_mount.as_os_str().is_empty(), "the listing hands the loop no mount to report against");
    }
    #[test]
    fn a_list_dir_says_when_the_directory_cannot_be_written() {
        use std::os::unix::fs::PermissionsExt;
        let sandbox = crate::backend::testdir::TestDir::new("listw");
        let locked = sandbox.dir("locked");
        sandbox.file("locked/a.txt", "a");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).unwrap();
        let (tx, _rx) = channel::<crate::backend::events::Event>();
        let watch = crate::backend::watch::Watch::start(tx.clone());
        let tb = crate::backend::state::Tables::load();
        let line = format!(r#"{{"c":"list","path":"{}","first":10}}"#, locked.to_string_lossy());
        let done = match list_dir(
            locked.to_string_lossy().to_string(), false, 10, line,
            std::sync::Arc::clone(&tb.mime), watch.raw_fd(), tx.clone(), &mount_body(),
        ) {
            Ok(done) => done,
            Err(failed) => panic!("a readable locked directory lists: {}", failed.error.msg),
        };
        assert!(!done.writable, "a 0o555 directory answers not writable");
        let (results, _drained) = channel::<crate::backend::thumbs::Done>();
        let pool = crate::backend::thumbs::Pool::new(
            1, results, crate::backend::thumbcache::default_root(),
            std::sync::Arc::clone(&tb.aliases), std::sync::Arc::clone(&tb.thumbs),
        );
        let mut st = crate::backend::state::State::new(crate::backend::dirsizeworker::Worker::new(tx));
        let mut out = Vec::<u8>::new();
        crate::backend::run::adopt_listed(&mut out, &mut st, &pool, &tb, &locked.to_string_lossy(), done, false);
        let text = String::from_utf8(out).expect("the wire is utf-8");
        assert!(text.contains(r#""w":false"#), "the listed line carries the worker's writability: {}", text);
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn a_dead_worker_is_an_operation_that_stopped_without_answering() {
        assert_eq!(internal_failure("rename", Path::new("/a")).msg, "the operation stopped without answering.");
    }
}
