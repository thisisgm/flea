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
    let (tx, _rx) = crate::backend::events::channel::<crate::backend::events::Event>();
    let watch = crate::backend::watch::Watch::start(tx.clone());
    let mime = std::sync::Arc::new(crate::backend::mime::Db::load());
    let failed = match list_dir(file.to_string_lossy().to_string(), false, 10, String::new(), mime, watch.raw_fd(), tx, &mount_body()) {
        Err(failed) => failed,
        Ok(_) => panic!("a file is not a listing"),
    };
    assert!(failed.watch_wd >= 0, "the loop must remove what the worker armed");
}
// A scan the loop gave up on armed its expanded children too, so their descriptors go back with the folder's.
#[test]
fn a_late_scan_hands_back_its_expanded_children_with_the_folder() {
    let sandbox = crate::backend::testdir::TestDir::new("listlate");
    let child = sandbox.dir("child");
    let (tx, rx) = crate::backend::events::channel::<crate::backend::events::Event>();
    let watch = crate::backend::watch::Watch::start(tx.clone());
    let mime = std::sync::Arc::new(crate::backend::mime::Db::load());
    let line = format!(r#"{{"c":"list","outlinePaths":["{}"]}}"#, crate::json::escape(&child.to_string_lossy()));
    let done = match list_dir(sandbox.path().to_string_lossy().to_string(), false, 10, line, mime, watch.raw_fd(), tx.clone(), &mount_body()) {
        Ok(done) => done,
        Err(failed) => panic!("a directory lists: {}", failed.error.msg),
    };
    let mut armed = done.listing.outline_watches.clone();
    armed.push(done.watch_wd);
    assert!(armed.len() == 2 && armed.iter().all(|&wd| wd >= 0), "the folder and its expanded child are both armed: {armed:?}");
    hand_back_late(&Ok(done), &tx);
    let error = FleaError { where_: "scan".to_string(), path: String::new(), msg: String::new() };
    hand_back_late(&Err(ListErr { error, mode: 0, watch_wd: -1 }), &tx);
    let back: Vec<i32> = rx.try_iter().filter_map(|event| match event {
        crate::backend::events::Event::AbandonWatch(wd) => Some(wd),
        _ => None,
    }).collect();
    assert_eq!(back, armed, "every armed descriptor goes back once, and a failure that armed none sends nothing");
}
#[test]
fn a_list_dir_carries_the_mount_its_worker_computed() {
    test_reset();
    let sandbox = crate::backend::testdir::TestDir::new("listmnt");
    let (tx, _rx) = crate::backend::events::channel::<crate::backend::events::Event>();
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
    let (tx, _rx) = crate::backend::events::channel::<crate::backend::events::Event>();
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
