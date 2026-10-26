use super::*;
use crate::backend::searchwalkhold::{is_delta, wait_walk_landed};
use crate::backend::{iomount, testdir::TestDir};
use std::time::Duration;

fn body(d: &TestDir, kind: &str) -> String {
    format!("1 0 8:1 / / rw - ext4 /dev/a rw\n30 1 0:45 / {} rw - {kind} server:/share rw\n", d.join("nas").display())
}

#[test]
fn search_mount_longest_boundary_preserves_local_islands_raw_paths_and_stack_order() {
    let d = TestDir::new("mountclass");
    let body = format!("{}31 30 8:2 / {} rw - vfat /dev/usb rw\n32 1 0:46 / {}/raw\\377\\040share rw - fuse.rclone remote: rw\n",
        body(&d, "nfs"), d.join("nas/island").display(), d.path().display());
    let reader = super::super::searchwalkread::Reader::new(&body);
    assert!(reader.is_remote(&d.join("nas/uncreated/deep")));
    assert!(!reader.is_remote(&d.join("nas/island/uncreated")));
    assert!(!reader.is_remote(&d.join("nasprefix")));
    use std::os::unix::ffi::OsStrExt;
    let raw = d.path().join(std::ffi::OsStr::from_bytes(b"raw\xff share/child"));
    assert!(reader.is_remote(&raw));
    let stacked = format!("{body}33 1 8:3 / {} rw - ext4 /dev/b rw\n", d.join("nas").display());
    assert!(!super::super::searchwalkread::Reader::new(&stacked).is_remote(&d.join("nas/child")));
}

#[test]
fn search_mount_root_remote_has_no_recursive_watch_and_preserves_raw_hidden_and_links() {
    use std::os::unix::ffi::OsStrExt;
    let d = TestDir::new("remotepolicy");
    d.dir("nas/sub");
    d.file("nas/sub/needle", "");
    d.file("nas/.hidden", "");
    let raw = d.join("nas").join(std::ffi::OsStr::from_bytes(b"raw\xff"));
    d.assert_contains(&raw);
    std::fs::write(&raw, "").unwrap();
    std::os::unix::fs::symlink(d.join("nas"), d.join("nas/sub/back")).unwrap();
    for hidden in [false, true] {
        let probes = Arc::new(WalkProbes::new());
        *probes.body.lock().unwrap() = Some(body(&d, "nfs"));
        let root = d.join("nas");
        let root = root.to_str().unwrap();
        let (tx, _) = crate::backend::events::channel();
        let mut session = Session::open_async_with_probes(root, root, hidden, tx, Arc::clone(&probes));
        session.join_walk();
        assert!(session.watch.is_none());
        assert_eq!(probes.remote_reads.load(Ordering::Relaxed), 2);
        let index = session.index.lock().unwrap();
        let mut actual: Vec<_> = (0..index.len()).map(|row| (index.raw(row), index.is_dir(row))).collect();
        let mut expected = vec![(b"sub".to_vec(), true), (b"sub/needle".to_vec(), false), (b"raw\xff".to_vec(), false), (b"sub/back".to_vec(), false)];
        if hidden { expected.push((b".hidden".to_vec(), false)); }
        actual.sort();
        expected.sort();
        assert_eq!(actual, expected);
        assert_eq!(index.partial, None);
    }
}

#[test]
fn search_mount_cancel_during_nested_remote_read_closes_without_waiting_for_it() {
    let d = TestDir::new("nestedcancel");
    d.dir("nas/deep");
    for row in 0..100 { d.file(&format!("nas/file{row}"), ""); }
    d.file("nas/deep/never-read", "");
    let probes = Arc::new(WalkProbes::new());
    *probes.body.lock().unwrap() = Some(body(&d, "fuse.sshfs"));
    let (parked, release) = probes.test_hold_scan_in("nas", 1);
    let (tx, rx) = crate::backend::events::channel();
    let root = d.path().to_str().unwrap();
    let mut session = Session::open_async_with_probes(root, root, false, tx, Arc::clone(&probes));
    parked.recv_timeout(Duration::from_secs(10)).unwrap();
    let started = Instant::now();
    assert!(session.cancel_walk().is_some());
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(session.partial_reason(), Some("cancelled"));
    assert!(session.watch.is_none());
    release.send(()).unwrap();
    wait_walk_landed(&rx, &session, Duration::from_secs(10), is_delta);
    assert_eq!(probes.remote_reads.load(Ordering::Relaxed), 1);
    assert_eq!(session.answer("never-read", 10).total, 0);
}

#[test]
fn search_mount_one_deadline_serializes_remote_reads_and_skips_the_failed_mount() {
    let d = TestDir::new("mountdeadline");
    d.dir("nas/a");
    d.file("nas/a/first", "");
    d.dir("nas/b");
    d.file("local", "");
    let probes = WalkProbes::new();
    let (parked, release) = probes.test_hold_scan_in("a", 1);
    let body = body(&d, "nfs");
    let reader = super::super::searchwalkread::Reader::new(&body);
    let started = Instant::now();
    std::thread::scope(|scope| {
        let first = scope.spawn(|| reader.read(&d.join("nas/a"), false, false, &probes));
        parked.recv_timeout(Duration::from_secs(10)).unwrap();
        assert!(reader.read(d.path(), true, false, &probes).is_ok(), "local work continues beside the held server");
        assert!(reader.read(&d.join("nas/b"), false, false, &probes).is_err());
        assert!(first.join().unwrap().is_err());
        assert!(started.elapsed() >= iomount::CALL_DEADLINE);
        assert!(started.elapsed() < iomount::CALL_DEADLINE + Duration::from_secs(5));
        for _ in 0..100 { assert!(reader.read(&d.join("nas/b"), false, false, &probes).is_err()); }
        assert_eq!(probes.remote_reads.load(Ordering::Relaxed), 1, "one deadline, never a worker for every sibling");
    });
    release.send(()).unwrap();
}

#[test]
fn search_mount_abandoned_remote_read_stops_before_its_next_entry() {
    use std::sync::mpsc;
    let d = TestDir::new("readdeadline");
    d.dir("nas");
    for row in 0..100 { d.file(&format!("nas/file{row}"), ""); }
    let (parked, waiting) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    let (finished, finish) = mpsc::channel::<()>();
    let entries = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&entries);
    let path = d.join("nas");
    let body = body(&d, "nfs");
    let worker = std::thread::spawn(move || crate::backend::searchread::bounded_observed(&path, &body, false, || false, move |_| {
        let _keep_until_read_ends = &finished;
        if observed.fetch_add(1, Ordering::Relaxed) == 0 {
            parked.send(()).unwrap();
            let _ = resume.recv();
        }
    }));
    waiting.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(worker.join().unwrap().is_err());
    assert_eq!(entries.load(Ordering::Relaxed), 1);
    release.send(()).unwrap();
    assert!(matches!(finish.recv_timeout(Duration::from_secs(10)), Err(mpsc::RecvTimeoutError::Disconnected)));
    assert_eq!(entries.load(Ordering::Relaxed), 1, "the expired worker performs no continuing entry or DT_UNKNOWN stat loop");
}

#[test]
fn search_mount_reopening_a_cancelled_scope_does_not_spawn_overlapping_mount_readers() {
    let d = TestDir::new("mountreopen");
    let path = d.dir("nas");
    d.file("nas/first", "");
    let body = body(&d, "nfs");
    let (parked, waiting) = std::sync::mpsc::channel();
    let (release, resume) = std::sync::mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancel);
    let owned = path.clone();
    let table = body.clone();
    let first = std::thread::spawn(move || crate::backend::searchread::bounded_observed(&owned, &table, false,
        move || flag.load(Ordering::Relaxed), move |_| { parked.send(()).unwrap(); let _ = resume.recv(); }));
    waiting.recv_timeout(Duration::from_secs(10)).unwrap();
    cancel.store(true, Ordering::Relaxed);
    iomount::test_reset();
    for _ in 0..100 {
        assert!(crate::backend::searchread::bounded(&path, &body, false).is_err(), "a closed or cancelled scope may still have a syscall in flight");
    }
    assert_eq!(iomount::test_calls(), 0, "overlap refusal must happen before another bounded worker is created");
    release.send(()).unwrap();
    assert_eq!(first.join().unwrap().unwrap().unwrap().partial, Some("cancelled"));
    assert!(crate::backend::searchread::bounded(&path, &body, false).unwrap().is_ok(), "the next request works once the old syscall returns");
}

#[test]
fn search_mount_nested_remote_refusal_keeps_local_rows_and_reports_partial() {
    for kind in ["nfs", "cifs", "fuse.sshfs"] {
        let d = TestDir::new("nestedrefusal");
        d.dir("nas/deep");
        d.file("nas/deep/unreachable", "");
        d.dir("local");
        d.file("local/visible", "");
        iomount::mark_stuck(&d.join("nas"));
        let (index, dirs) = walk(d.path(), false, false, &body(&d, kind), &WalkProbes::new());
        let mut paths: Vec<_> = (0..index.len()).map(|row| index.path(row)).collect();
        paths.sort();
        assert_eq!(index.partial, Some("unreadable"), "a nested {kind} refusal cannot certify coverage");
        assert_eq!(paths, ["local", "local/visible", "nas"]);
        assert_eq!(dirs, 2, "only the root and local directory were read");
    }
}

#[test]
fn search_mount_missing_local_root_reports_the_read_refusal_before_watch_failure() {
    let d = TestDir::new("missingmountroot");
    let missing = d.join("missing");
    let root = missing.to_str().unwrap();
    let (tx, _) = crate::backend::events::channel();
    let mut session = Session::open_async(root, root, false, tx);
    session.join_walk();
    assert_eq!(session.partial_reason(), Some("unreadable"));
    assert_eq!(session.indexed(), 0);
}

#[test]
fn search_mount_walk_classifies_only_the_scope_and_each_mount_boundary() {
    let d = TestDir::new("mountonce");
    for a in 0..3 {
        for b in 0..3 {
            d.dir(&format!("a{a}/b{b}"));
            d.file(&format!("a{a}/b{b}/leaf"), "");
        }
    }
    for dir in ["nas/island", "nas/far", "x/gvfs-like/y"] { d.dir(dir); }
    d.file("nas/island/kept", "");
    d.file("nas/far/away", "");
    d.file("x/gvfs-like/y/z", "");
    let body = format!("{}31 30 8:2 / {} rw - vfat /dev/usb rw\n", body(&d, "nfs"), d.join("nas/island").display());
    let reader = super::super::searchwalkread::Reader::new(&body);
    let index = Mutex::new(Index::new());
    let probes = WalkProbes::new();
    let opened = walk_local(&index, d.path(), false, &body, &reader, &probes);
    // The scope, then nas and nas/island where mounts start, then x/gvfs-like and x/gvfs-like/y, whose text holds "gvfs".
    assert_eq!(reader.classified.load(Ordering::Relaxed), 5, "{opened} directories, classified once per boundary");
    assert!(reader.is_remote(&d.join("nas/far")), "a child below a remote mount stays remote");
    assert_eq!(reader.class(&d.join("nas/far"), Some(&reader.class(&d.join("nas"), None))), Some(d.join("nas")));
    assert_eq!(reader.class(&d.join("nas/island"), Some(&Some(d.join("nas")))), None, "a local island under a remote mount is local");
    assert_eq!(reader.class(&d.join("a0/b0"), Some(&None)), None);
    let paths: Vec<String> = { let index = index.lock().unwrap(); (0..index.len()).map(|row| index.path(row)).collect() };
    for want in ["a2/b2/leaf", "nas/island/kept", "nas/far/away", "x/gvfs-like/y/z"] {
        assert!(paths.iter().any(|path| path == want), "{want} lands");
    }
}
