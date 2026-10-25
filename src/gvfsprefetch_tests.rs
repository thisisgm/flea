// src/gvfsprefetch.rs's tests, kept beside it so that file stays a mechanism file.
use super::*;
use crate::backend::testdir::TestDir;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::sync::{Mutex, MutexGuard};

// Held by the tests that touch the process-wide CONSUMED flag; no test writes the environment.
static CONSUMED_LOCK: Mutex<()> = Mutex::new(());

// A literal gvfs path, so is_gvfs answers without reading the environment.
const SHARE: &str = "/run/user/1000/gvfs/smb-share:server=t,share=u/dir";
// Sample gio output, two rows; the hidden one is dropped unless the scan asks for it.
const TWO_ROWS: &str = "smb://h/share/a.txt\t3\t(regular)\ttime::modified=100\nsmb://h/share/.hidden\t1\t(regular)\ttime::modified=100\n";

fn runtime_fixture(tag: &str) -> (TestDir, PathBuf) {
    let dir = TestDir::new(tag);
    let runtime = dir.path().join("runtime/flea");
    std::fs::create_dir_all(&runtime).unwrap();
    std::fs::set_permissions(&runtime, std::fs::Permissions::from_mode(0o700)).unwrap();
    (dir, runtime)
}

fn write_prefetch(runtime: &Path, name: &str, body: &str) -> PathBuf {
    let dest = runtime.join(name);
    std::fs::write(&dest, body).unwrap();
    dest
}

fn fake_gio(dir: &TestDir, name: &str, body: &str) -> String {
    dir.script(name, body).to_string_lossy().into_owned()
}

fn live_deadline() -> SystemTime {
    SystemTime::now() + Duration::from_secs(15)
}

fn short_deadline() -> SystemTime {
    SystemTime::now() + Duration::from_millis(300)
}

// A launch start just before now, so a file written now reads as newer than the launch.
fn fresh_start_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64 - 1000
}

// The once flag is process-wide, so every test that spends or reads it holds this for its whole run.
fn hold_consumed() -> MutexGuard<'static, ()> {
    CONSUMED_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn reset_consumed() {
    CONSUMED.store(false, Ordering::SeqCst);
}

#[test]
fn a_file_published_while_waiting_is_not_refused_as_stale() {
    let (_dir, runtime) = runtime_fixture("gvfs-wait-publish");
    let dest = runtime.join("gvfs-1.list");
    let start_ms = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;
    let writer = std::thread::spawn({
        let dest = dest.clone();
        move || {
            std::thread::sleep(Duration::from_millis(200));
            std::fs::write(&dest, TWO_ROWS).unwrap();
        }
    });
    let out = adopt_in(SHARE, false, &dest, live_deadline(), start_ms, Some(&runtime));
    let _ = writer.join();
    assert!(out.is_some(), "a file published while the backend waits adopts, got None");
    assert!(!dest.exists(), "the dest is claimed away so a second reader finds the claim");
    assert!(claimed_path(&dest).exists(), "the claim stays for the second reader and the sweeper");
    assert_eq!(std::fs::metadata(claimed_path(&dest)).unwrap().len(), 0, "the claim is truncated to its marker");
}

#[test]
fn a_failing_gio_publishes_a_marker_and_adoption_ends_at_once() {
    let (dir, runtime) = runtime_fixture("gvfs-fail-fast");
    let gio = fake_gio(&dir, "gio", "#!/bin/sh\nexit 1\n");
    let dest = runtime.join("gvfs-9.list");
    assert_eq!(run_in(SHARE, &dest, &gio, Some(&runtime)), 1);
    assert_eq!(std::fs::read(&dest).unwrap(), FAIL_MARKER);
    let t = Instant::now();
    let start_ms = dest.metadata().unwrap().modified().unwrap().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64 - 500;
    assert!(adopt_in(SHARE, false, &dest, live_deadline(), start_ms, Some(&runtime)).is_none());
    assert!(t.elapsed() < Duration::from_secs(1), "a failure marker ends the wait, not the 15 s deadline");
    assert_eq!(std::fs::metadata(claimed_path(&dest)).unwrap().len(), 0, "the claim is truncated to its marker");
}

#[test]
fn a_second_reader_finds_the_claim_and_returns_at_once() {
    let (_dir, runtime) = runtime_fixture("gvfs-second");
    let dest = write_prefetch(&runtime, "gvfs-1.list", TWO_ROWS);
    let t = Instant::now();
    let first = adopt_in(SHARE, false, &dest, live_deadline(), 0, Some(&runtime));
    assert!(first.is_some(), "the first reader adopts");
    let second = adopt_in(SHARE, false, &dest, live_deadline(), 0, Some(&runtime));
    assert!(second.is_none(), "the second reader finds the claimed sibling, got Some");
    assert!(t.elapsed() < Duration::from_secs(5), "the second reader never waits out the deadline");
    assert!(!dest.exists() && claimed_path(&dest).exists(), "claim, not delete: dest gone, claim kept");
    assert_eq!(std::fs::metadata(claimed_path(&dest)).unwrap().len(), 0, "the claim is truncated to its marker");
}

#[test]
fn a_completed_prefetch_adopts_through_scan_with_no_gio_child() {
    let _lock = hold_consumed();
    reset_consumed();
    let dir = TestDir::new("gvfs-adopt");
    let runtime = dir.path().join("runtime/flea");
    std::fs::create_dir_all(&runtime).unwrap();
    std::fs::set_permissions(&runtime, std::fs::Permissions::from_mode(0o700)).unwrap();
    let runtime_root = dir.path().join("runtime");
    let marker = dir.path().join("gio-ran");
    let gio = fake_gio(&dir, "gio", &format!("#!/bin/sh\ntouch {}\nexit 1\n", marker.display()));
    let dest = write_prefetch(&runtime, "gvfs-1.list", TWO_ROWS);
    let mtime_ms = dest.metadata().unwrap().modified().unwrap().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;
    let prefetch = Prefetch { dest: dest.clone(), path: SHARE.to_string(), start_ms: mtime_ms - 1000 };
    let (listing, _) = crate::backend::scan::scan_in(SHARE, false, Some(prefetch), &gio, &|| Some(runtime_root.join("flea"))).unwrap();
    assert_eq!((listing.len(), listing.name(0)), (1, "a.txt"), "the hidden row is filtered at parse time");
    assert!(!marker.exists(), "adoption through scan_with spawns no gio child at all");
    assert!(!dest.exists(), "the file is claimed on read and never read again");
    assert_eq!(std::fs::metadata(claimed_path(&dest)).unwrap().len(), 0, "the claim is truncated to its marker");
    reset_consumed();
}

#[test]
fn a_hidden_scan_keeps_what_a_plain_scan_filters() {
    let (_dir, runtime) = runtime_fixture("gvfs-hidden");
    let dest = write_prefetch(&runtime, "gvfs-1.list", TWO_ROWS);
    let (listing, _) = adopt_in(SHARE, true, &dest, live_deadline(), 0, Some(&runtime)).unwrap();
    assert_eq!((listing.len(), listing.name(1)), (2, ".hidden"));
}

#[test]
fn a_stale_file_is_refused_and_kept_at_its_original_path() {
    let (_dir, runtime) = runtime_fixture("gvfs-stale");
    let dest = write_prefetch(&runtime, "gvfs-1.list", TWO_ROWS);
    let mtime_ms = dest.metadata().unwrap().modified().unwrap().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;
    let start_ms = mtime_ms + (STALE_SECS + 10) * 1000;
    assert!(adopt_in(SHARE, false, &dest, live_deadline(), start_ms, Some(&runtime)).is_none());
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), TWO_ROWS);
    assert!(!claimed_path(&dest).exists());
}

// Runs adopt_in on a thread so a wait that never ends fails the test instead of hanging it; answers the result and how long it took.
fn adopt_within(dest: PathBuf, runtime: PathBuf, deadline: SystemTime, bound: Duration) -> (Option<bool>, Duration) {
    let (tx, rx) = std::sync::mpsc::channel();
    let start_ms = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;
    let t = Instant::now();
    std::thread::spawn(move || {
        let _ = tx.send(adopt_in(SHARE, false, &dest, deadline, start_ms, Some(&runtime)).is_some());
    });
    (rx.recv_timeout(bound).ok(), t.elapsed())
}

#[test]
fn a_fresh_launch_whose_child_never_publishes_stops_at_the_deadline() {
    let (_dir, runtime) = runtime_fixture("gvfs-loop-deadline");
    let (answer, took) = adopt_within(runtime.join("gvfs-never.list"), runtime, short_deadline(), Duration::from_secs(3));
    assert_eq!(answer, Some(false), "inside the grace window only the loop's deadline can end the wait");
    assert!(took >= Duration::from_millis(250), "an answer after {:?} came from an early return, not the 300 ms deadline", took);
}

#[test]
fn a_claim_landing_mid_wait_ends_the_wait() {
    let (_dir, runtime) = runtime_fixture("gvfs-loop-claim");
    let dest = runtime.join("gvfs-1.list");
    let claimer = std::thread::spawn({
        let claimed = claimed_path(&dest);
        move || {
            std::thread::sleep(Duration::from_millis(100));
            std::fs::write(claimed, TWO_ROWS).unwrap();
        }
    });
    let (answer, took) = adopt_within(dest, runtime, live_deadline(), Duration::from_secs(3));
    let _ = claimer.join();
    assert_eq!(answer, Some(false), "another reader's claim ends the wait long before the 15 s deadline");
    assert!(took >= Duration::from_millis(80), "an answer after {:?} came before the claim landed at 100 ms", took);
}

#[test]
fn a_fifo_at_the_dest_never_blocks_the_scan() {
    let (_dir, runtime) = runtime_fixture("gvfs-fifo");
    let dest = runtime.join("gvfs-1.list");
    assert!(std::process::Command::new("mkfifo").arg(&dest).status().unwrap().success());
    // A held reader lets a non-blocking write-open succeed, and Linux raises POLLHUP on it once that writer closes.
    let reader = std::fs::OpenOptions::new().read(true).custom_flags(crate::oflags::O_NONBLOCK).open(&dest).unwrap();
    let (answer, took) = adopt_within(dest.clone(), runtime, short_deadline(), Duration::from_secs(5));
    assert_eq!(answer, Some(false), "a fifo at the dest adopts nothing");
    assert!(took < Duration::from_secs(5), "the scan answers inside the bound instead of hanging");
    let kind = std::fs::symlink_metadata(&dest).unwrap().file_type();
    assert!(kind.is_fifo(), "the fifo is never truncated or replaced");
    let mut fds = crate::backend::child::PollFd { fd: reader.as_raw_fd(), events: 0, revents: 0 };
    assert_eq!(unsafe { crate::backend::child::poll(&mut fds, 1, 0) }, 0, "the claim is never opened for write, revents {}", fds.revents);
}

#[test]
fn a_file_published_after_launch_but_older_than_stale_secs_is_refused() {
    let (_dir, runtime) = runtime_fixture("gvfs-aged");
    let dest = write_prefetch(&runtime, "gvfs-1.list", TWO_ROWS);
    let aged = format!("{} seconds ago", STALE_SECS + 10);
    assert!(std::process::Command::new("touch").arg("-d").arg(&aged).arg(&dest).status().unwrap().success());
    let start_ms = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64 - (STALE_SECS + 50) * 1000;
    assert!(adopt_in(SHARE, false, &dest, live_deadline(), start_ms, Some(&runtime)).is_none(), "newer than the launch but aged past STALE_SECS");
}

#[test]
fn a_missing_file_past_the_deadline_answers_at_once() {
    let (_dir, runtime) = runtime_fixture("gvfs-missing");
    let t = Instant::now();
    assert!(adopt_in(SHARE, false, &runtime.join("gvfs-absent.list"), UNIX_EPOCH, 0, Some(&runtime)).is_none());
    assert!(t.elapsed() < Duration::from_secs(5), "no wait past a deadline that already passed");
}

#[test]
fn a_temp_file_alone_is_a_partial_write_and_adopts_nothing() {
    let (_dir, runtime) = runtime_fixture("gvfs-partial");
    let dest = runtime.join("gvfs-1.list");
    let tmp = runtime.join("gvfs-1.list.123.tmp");
    std::fs::write(&tmp, TWO_ROWS).unwrap();
    let t = Instant::now();
    assert!(adopt_in(SHARE, false, &dest, short_deadline(), 0, Some(&runtime)).is_none());
    assert!(t.elapsed() < Duration::from_secs(5), "a missing dest with a short deadline never waits 15 s");
    assert!(tmp.exists(), "the temp file is never read, only dest and its claim are");
    assert!(!dest.exists() && !claimed_path(&dest).exists(), "nothing is claimed from a partial write");
}

#[test]
fn a_world_readable_runtime_dir_is_refused() {
    let (_dir, runtime) = runtime_fixture("gvfs-dir755");
    std::fs::set_permissions(&runtime, std::fs::Permissions::from_mode(0o755)).unwrap();
    let dest = write_prefetch(&runtime, "gvfs-1.list", TWO_ROWS);
    assert!(adopt_in(SHARE, false, &dest, live_deadline(), 0, Some(&runtime)).is_none());
    assert!(dest.exists() && !claimed_path(&dest).exists(), "a dir that is not ours is refused before any rename");
}

#[test]
fn a_symlinked_dest_is_refused_rather_than_followed() {
    let (_dir, runtime) = runtime_fixture("gvfs-symlink");
    let real = write_prefetch(&runtime, "real.list", TWO_ROWS);
    let dest = runtime.join("gvfs-1.list");
    std::os::unix::fs::symlink(&real, &dest).unwrap();
    assert!(adopt_in(SHARE, false, &dest, live_deadline(), 0, Some(&runtime)).is_none());
    let _ = real;
}

#[test]
fn a_file_outside_the_runtime_dir_is_not_this_launch() {
    let (dir, runtime) = runtime_fixture("gvfs-outside");
    let elsewhere = dir.path().join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    let dest = elsewhere.join("gvfs-1.list");
    std::fs::write(&dest, TWO_ROWS).unwrap();
    assert!(adopt_in(SHARE, false, &dest, live_deadline(), 0, Some(&runtime)).is_none());
    assert!(dest.exists() && !claimed_path(&dest).exists(), "a file that is not this launch's is left exactly where it is");
}

#[test]
fn the_subcommand_publishes_gio_bytes_exclusively_at_0600() {
    let (dir, runtime) = runtime_fixture("gvfs-run-ok");
    let gio = fake_gio(&dir, "gio", "#!/bin/sh\nprintf 'smb://h/share/a.txt\\t3\\t(regular)\\ttime::modified=100\\n'\n");
    let dest = runtime.join("gvfs-9.list");
    assert_eq!(run_in(SHARE, &dest, &gio, Some(&runtime)), 0);
    assert!(dest.is_file());
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), "smb://h/share/a.txt\t3\t(regular)\ttime::modified=100\n");
    assert_eq!(dest.metadata().unwrap().permissions().mode() & 0o777, 0o600);
    assert_eq!(std::fs::read_dir(&runtime).unwrap().count(), 1, "no temp file is left behind");
}

#[test]
fn the_subcommand_publishes_a_failure_marker_on_gio_failure() {
    let (dir, runtime) = runtime_fixture("gvfs-run-fail");
    let gio = fake_gio(&dir, "gio", "#!/bin/sh\nexit 1\n");
    let dest = runtime.join("gvfs-9.list");
    assert_eq!(run_in(SHARE, &dest, &gio, Some(&runtime)), 1);
    assert_eq!(std::fs::read(&dest).unwrap(), FAIL_MARKER, "a failed gio leaves a marker, not nothing");
    assert_eq!(dest.metadata().unwrap().permissions().mode() & 0o777, 0o600);
}

#[test]
fn the_subcommand_refuses_a_dest_outside_the_runtime_dir() {
    let (dir, runtime) = runtime_fixture("gvfs-run-dest");
    let gio = fake_gio(&dir, "gio", "#!/bin/sh\nexit 0\n");
    let dest = dir.path().join("gvfs-9.list");
    assert_eq!(run_in(SHARE, &dest, &gio, Some(&runtime)), 2);
    assert!(!dest.exists());
}

#[test]
fn the_subcommand_refuses_a_local_path_and_a_bad_dir() {
    let (dir, runtime) = runtime_fixture("gvfs-run-refuse");
    let gio = fake_gio(&dir, "gio", "#!/bin/sh\nexit 0\n");
    assert_eq!(run_in("/home/gm", &runtime.join("gvfs-9.list"), &gio, Some(&runtime)), 2);
    std::fs::set_permissions(&runtime, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(run_in(SHARE, &runtime.join("gvfs-9.list"), &gio, Some(&runtime)), 2);
}

#[test]
fn the_sweeper_reaps_only_old_prefetch_files() {
    let (_dir, runtime) = runtime_fixture("gvfs-sweep");
    let old = write_prefetch(&runtime, "gvfs-old.list", TWO_ROWS);
    assert!(std::process::Command::new("touch").arg("-d").arg("70 seconds ago").arg(&old).status().unwrap().success());
    let claimed_old = claimed_path(&old);
    std::fs::write(&claimed_old, TWO_ROWS).unwrap();
    assert!(std::process::Command::new("touch").arg("-d").arg("70 seconds ago").arg(&claimed_old).status().unwrap().success());
    let fresh = write_prefetch(&runtime, "gvfs-fresh.list", TWO_ROWS);
    let other = write_prefetch(&runtime, "notes.txt", TWO_ROWS);
    sweep_dir(&runtime, SystemTime::now());
    assert!(!old.exists(), "a leftover older than a minute goes");
    assert!(!claimed_old.exists(), "a claimed leftover older than a minute goes by the same age rule");
    assert!(fresh.exists(), "this launch's file stays");
    assert!(other.exists(), "a name this launcher never writes stays");
}

#[test]
fn a_different_path_never_spends_the_once_flag() {
    let _lock = hold_consumed();
    reset_consumed();
    let (_dir, runtime) = runtime_fixture("gvfs-other-path");
    let dest = write_prefetch(&runtime, "gvfs-1.list", TWO_ROWS);
    let mtime_ms = dest.metadata().unwrap().modified().unwrap().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;
    let other = Prefetch { dest: dest.clone(), path: SHARE.to_string(), start_ms: mtime_ms - 1000 };
    assert!(adopt_matching_in("/run/user/1000/gvfs/smb-share:server=t,share=u/other", false, &other, Some(&runtime)).is_none());
    assert!(!CONSUMED.load(Ordering::SeqCst), "a path that never matched spends no once flag");
    assert!(dest.exists(), "another path takes today's gio path and leaves the file");
    let matching = Prefetch { dest: dest.clone(), path: SHARE.to_string(), start_ms: mtime_ms - 1000 };
    assert!(adopt_matching_in(SHARE, false, &matching, Some(&runtime)).is_some(), "the matching path still adopts after the miss");
    reset_consumed();
}

#[test]
fn the_scan_adopts_once_then_falls_back_to_gio() {
    let _lock = hold_consumed();
    reset_consumed();
    let dir = TestDir::new("gvfs-once");
    let runtime = dir.path().join("runtime/flea");
    std::fs::create_dir_all(&runtime).unwrap();
    std::fs::set_permissions(&runtime, std::fs::Permissions::from_mode(0o700)).unwrap();
    let runtime_root = dir.path().join("runtime");
    let marker = dir.path().join("gio-ran");
    let gio = fake_gio(&dir, "gio", &format!("#!/bin/sh\ntouch {}\nprintf 'smb://h/share/from-gio.txt\\t5\\t(regular)\\ttime::modified=7\\n'\n", marker.display()));
    let start_ms = fresh_start_ms();
    let dest = runtime.join("gvfs-1.list");
    std::fs::write(&dest, TWO_ROWS).unwrap();
    let prefetch = Prefetch { dest: dest.clone(), path: SHARE.to_string(), start_ms };
    let (first, _) = crate::backend::scan::scan_in(SHARE, false, Some(prefetch), &gio, &|| Some(runtime_root.join("flea"))).unwrap();
    assert_eq!((first.len(), first.name(0)), (1, "a.txt"), "the first scan of the path spawns no gio child");
    assert!(!marker.exists());
    std::fs::write(&dest, TWO_ROWS).unwrap();
    let again = Prefetch { dest, path: SHARE.to_string(), start_ms };
    let (second, _) = crate::backend::scan::scan_in(SHARE, false, Some(again), &gio, &|| Some(runtime_root.join("flea"))).unwrap();
    assert_eq!((second.len(), second.name(0)), (1, "from-gio.txt"));
    assert!(marker.exists(), "the second listing takes today's gio path");
    reset_consumed();
}

#[test]
fn the_launcher_arms_only_a_gvfs_path() {
    let dir = TestDir::new("gvfs-prepare");
    let root = dir.path().join("runtime");
    let runtime = runtime_dir_from(Some(root.clone().into_os_string()));
    assert_eq!(runtime, Some(root.join("flea")), "the session's runtime root names Flea's own dir under it");
    assert!(runtime_dir_from(Some("".into())).is_none() && runtime_dir_from(None).is_none(), "a session naming no runtime root names no dir");
    // prepare_for is the gate prepare runs with the session's dir, so both arms are driven here.
    assert!(prepare_for("/home/gm", runtime.clone()).is_none(), "a local launch does nothing new");
    assert!(!root.exists(), "a local launch makes no directory");
    assert!(prepare_for(SHARE, None).is_none(), "a gvfs launch in a session without a runtime dir arms nothing");
    let (dest, start_ms) = prepare_for(SHARE, runtime).expect("a gvfs launch names its file");
    assert!(start_ms > 0);
    assert_eq!(dest.parent(), Some(root.join("flea").as_path()));
    assert!(dest.file_name().unwrap().to_string_lossy().starts_with("gvfs-"));
    assert_eq!(dest.parent().unwrap().metadata().unwrap().permissions().mode() & 0o777, 0o700);
}

#[test]
fn the_environment_names_all_three_or_nothing() {
    let env = |start: &'static str, path: bool| move |name: &str| -> Option<std::ffi::OsString> {
        match name {
            PREFETCH_ENV => Some("/run/user/1000/flea/gvfs-1.list".into()),
            PATH_ENV if path => Some(SHARE.into()),
            START_ENV => Some(start.into()),
            _ => None,
        }
    };
    let named = env_prefetch_from(env("12345", true)).expect("three set variables read as one prefetch");
    assert_eq!((named.path.as_str(), named.start_ms), (SHARE, 12345));
    assert!(env_prefetch_from(env("not-a-number", true)).is_none(), "an unparsable start is no prefetch");
    assert!(env_prefetch_from(env("12345", false)).is_none(), "a missing path is no prefetch");
}

#[path = "gvfsprefetch_preserve_tests.rs"]
mod preserve;
