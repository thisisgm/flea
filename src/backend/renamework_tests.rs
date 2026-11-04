use super::*;
use crate::backend::testdir::TestDir;

#[test]
fn publisher_keeps_its_history_after_a_diagnostic_write_failure() {
    use std::io::{Read, Write};
    use std::os::unix::fs::PermissionsExt;
    use std::process::{Command, Stdio};
    const CHILD: &str = "FLEA_JOURNAL_DIAGNOSTIC_CHILD";
    const TEST: &str =
        "backend::renamework::tests::publisher_keeps_its_history_after_a_diagnostic_write_failure";
    if std::env::var_os(CHILD).is_none() {
        for broken in [false, true] {
            let mut child = Command::new("/usr/bin/setsid")
                .arg("-w")
                .arg(std::env::current_exe().unwrap())
                .env_clear()
                .env("HOME", std::env::var_os("HOME").unwrap())
                .env("TMPDIR", std::env::temp_dir())
                .args(["--exact", TEST, "--nocapture"])
                .env(CHILD, "1")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            if broken {
                drop(child.stderr.take());
            }
            child.stdin.take().unwrap().write_all(b"x").unwrap();
            let output = child.wait_with_output().unwrap();
            let stdout = String::from_utf8_lossy(&output.stdout);
            println!("broken_stderr={broken} status={}\n{stdout}", output.status);
            assert!(
                output.status.success(),
                "journal publication must return its history even when stderr is closed"
            );
            assert!(
                stdout.contains("running 1 test") && stdout.contains("1 passed"),
                "the child must run exactly the diagnostic case: {stdout}"
            );
            if !broken {
                let stderr = String::from_utf8_lossy(&output.stderr);
                assert!(
                    stderr.contains("flea: the shared undo lock could not be opened"),
                    "the intact run must write the diagnostic the broken run cannot: {stderr}"
                );
            }
        }
        return;
    }
    std::io::stdin().read_exact(&mut [0u8; 1]).unwrap();
    let root = TestDir::new("publication-stderr");
    let shared = root.dir("shared");
    root.assert_contains(&shared);
    std::fs::set_permissions(&shared, std::fs::Permissions::from_mode(0o700)).unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let mut ops = Ops::new(tx);
    ops.journal.attach_test_shared(shared.clone());
    let older = root.file("older", "previous history");
    ops.journal.push(Entry {
        op: "older".into(),
        steps: vec![Step::Created { path: older }],
    });
    let previous = std::fs::read(shared.join("undo-journal")).unwrap();
    let lock = shared.join("undo-journal.lock");
    assert!(lock.is_absolute() && !lock.as_os_str().is_empty());
    root.assert_contains(&lock);
    std::fs::remove_file(&lock).unwrap();
    std::fs::create_dir(&lock).unwrap();
    let source = root.file("before", "payload");
    root.assert_contains(&source);
    root.assert_contains(&root.join("after"));
    let outcome = super::super::ops::rename(&source, "after");
    assert!(outcome.is_ok());
    ops.journal.reserve(77);
    publish(&mut ops, Publication::Rename(77, outcome));
    let event = rx
        .recv_timeout(Duration::from_secs(2))
        .expect("publisher must send JournalDone after the shared-lock error");
    let mut out = Vec::new();
    super::super::opsdispatch::report_op(&mut out, &mut ops, event);
    assert!(ops.journal.deferred.is_none());
    assert!(String::from_utf8_lossy(&out).contains("\"t\":\"renamed\""));
    assert_eq!(
        std::fs::read(shared.join("undo-journal")).unwrap(),
        previous
    );
    root.assert_contains(&root.join("after"));
    root.assert_contains(&source);
    assert_eq!(ops.journal.undo().unwrap(), "rename");
    assert_eq!(std::fs::read_to_string(source).unwrap(), "payload");
}

fn receive(out: &mut Vec<u8>, ops: &mut Ops, rx: &std::sync::mpsc::Receiver<OpMsg>) {
    let event = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("worker completion");
    super::super::opsdispatch::report_op(out, ops, event);
}

#[test]
fn a_held_local_rename_reserves_its_order_before_later_writes() {
    let root = TestDir::new("rename-accepted-order");
    let source = root.file("before", "payload");
    let later = root.file("later", "later write");
    let (tx, rx) = std::sync::mpsc::channel();
    let mut ops = Ops::new(tx);
    let (release, wait) = std::sync::mpsc::channel();
    let from = source.clone();
    let mut out = Vec::new();
    super::super::opsdispatch::do_rename_with(
        &mut out,
        &mut ops,
        source.to_str().unwrap(),
        Duration::from_secs(10),
        move || {
            wait.recv().unwrap();
            super::super::ops::rename(&from, "after")
        },
    );
    assert!(source.exists() && !root.join("after").exists());
    assert_eq!(ops.pending.len(), 1);
    ops.journal.push(Entry {
        op: "later".into(),
        steps: vec![Step::Created {
            path: later.clone(),
        }],
    });
    assert!(
        ops.journal.is_empty(),
        "the later write stays behind the accepted rename"
    );
    super::super::opsdispatch::do_undo(&mut out, &mut ops);
    assert!(String::from_utf8_lossy(&out).contains("already running"));
    release.send(()).unwrap();
    receive(&mut out, &mut ops, &rx);
    assert!(ops.journal.deferred.is_some());
    receive(&mut out, &mut ops, &rx);
    assert_eq!(ops.journal.len(), 2);
    root.assert_contains(&later);
    assert_eq!(ops.journal.undo().unwrap(), "later");
    assert!(!later.exists() && root.join("after").exists());
    root.assert_contains(&source);
    root.assert_contains(&root.join("after"));
    assert_eq!(ops.journal.undo().unwrap(), "rename");
    assert_eq!(std::fs::read_to_string(source).unwrap(), "payload");
    assert!(
        ops.journal.undo().is_err(),
        "one rename makes exactly one undo entry"
    );
}

#[test]
fn publication_keeps_queued_entries_reservations_and_other_operation_owners() {
    let root = TestDir::new("rename-publication-order");
    let source = root.file("before", "payload");
    let shared = root.dir("shared");
    use std::os::unix::fs::PermissionsExt;
    root.assert_contains(&shared);
    std::fs::set_permissions(&shared, std::fs::Permissions::from_mode(0o700)).unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let mut ops = Ops::new(tx);
    ops.journal.attach_test_shared(shared.clone());
    let lock_path = shared.join("undo-journal.lock");
    root.assert_contains(&lock_path);
    let lock = std::fs::File::create(lock_path).unwrap();
    crate::uistore::lock_exclusive(&lock).unwrap();
    let mut out = Vec::new();
    super::super::opsdispatch::do_rename(&mut out, &mut ops, source.to_str().unwrap(), "after");
    receive(&mut out, &mut ops, &rx);
    assert!(ops.journal.deferred.is_some());
    assert!(
        rx.recv_timeout(Duration::from_millis(30)).is_err(),
        "publication is held by the real lock"
    );
    let middle = root.file("middle", "middle");
    let last = root.file("last", "last");
    ops.journal.reserve(900);
    ops.journal.push(Entry {
        op: "last".into(),
        steps: vec![Step::Created { path: last.clone() }],
    });
    ops.journal.fill(
        900,
        Entry {
            op: "middle".into(),
            steps: vec![Step::Created {
                path: middle.clone(),
            }],
        },
    );
    let (other, _) = ops.claim_transfer();
    ops.pending.push(SlowClaim {
        id: 901,
        path: "other".into(),
        mount: PathBuf::from("/other"),
    });
    crate::uistore::unlock(&lock).unwrap();
    while ops.journal.deferred.is_some() {
        receive(&mut out, &mut ops, &rx);
    }
    assert_eq!(
        ops.live.running(),
        Some(other),
        "a late rename never frees a newer operation slot"
    );
    assert_eq!(ops.pending.len(), 1);
    assert_eq!(
        ops.pending[0].id, 901,
        "a late rename removes only its own pending claim"
    );
    assert_eq!(
        String::from_utf8_lossy(&out)
            .matches("\"t\":\"renamed\"")
            .count(),
        1
    );
    root.assert_contains(&last);
    assert_eq!(ops.journal.undo().unwrap(), "last");
    root.assert_contains(&middle);
    assert_eq!(ops.journal.undo().unwrap(), "middle");
    assert!(!last.exists() && !middle.exists());
    root.assert_contains(&source);
    root.assert_contains(&root.join("after"));
    assert_eq!(ops.journal.undo().unwrap(), "rename");
    assert_eq!(std::fs::read_to_string(source).unwrap(), "payload");
    assert!(ops.journal.undo().is_err());
}

#[test]
fn a_rename_behind_a_trash_reservation_never_publishes_on_the_dispatcher() {
    let root = TestDir::new("rename-behind-trash");
    let source = root.file("before", "payload");
    let shared = root.dir("shared");
    use std::os::unix::fs::PermissionsExt;
    root.assert_contains(&shared);
    std::fs::set_permissions(&shared, std::fs::Permissions::from_mode(0o700)).unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let mut ops = Ops::new(tx);
    ops.journal.attach_test_shared(shared.clone());
    let mut out = Vec::new();
    super::super::opsdispatch::report_op(
        &mut out,
        &mut ops,
        OpMsg::TrashAnnounced {
            id: 600,
            path: source.to_string_lossy().into(),
            ok: 0,
            failed: 1,
            reason: String::new(),
            request: 0,
            moved: Vec::new(),
        },
    );
    super::super::opsdispatch::do_rename(&mut out, &mut ops, source.to_str().unwrap(), "after");
    receive(&mut out, &mut ops, &rx);
    receive(&mut out, &mut ops, &rx);
    assert!(String::from_utf8_lossy(&out).contains("\"t\":\"renamed\""));
    let lock_path = shared.join("undo-journal.lock");
    root.assert_contains(&lock_path);
    let lock = std::fs::File::create(lock_path).unwrap();
    crate::uistore::lock_exclusive(&lock).unwrap();
    let (returned, response) = std::sync::mpsc::channel();
    let dispatcher = std::thread::spawn(move || {
        super::super::opsdispatch::report_op(
            &mut out,
            &mut ops,
            OpMsg::Trashed {
                id: 600,
                request: 0,
                ok: 0,
                failed: 1,
                entry: Entry {
                    op: "trash".into(),
                    steps: Vec::new(),
                },
                reason: String::new(),
                announced: true,
            },
        );
        returned.send((ops, out)).unwrap();
    });
    let during_hold = response.recv_timeout(Duration::from_millis(500));
    let responsive = during_hold.is_ok();
    crate::uistore::unlock(&lock).unwrap();
    let (mut ops, mut out) =
        during_hold.unwrap_or_else(|_| response.recv_timeout(Duration::from_secs(5)).unwrap());
    dispatcher.join().unwrap();
    while ops.journal.deferred.is_some() {
        receive(&mut out, &mut ops, &rx);
    }
    root.assert_contains(&source);
    root.assert_contains(&root.join("after"));
    assert_eq!(ops.journal.undo().unwrap(), "rename");
    assert_eq!(std::fs::read_to_string(source).unwrap(), "payload");
    assert!(ops.journal.undo().is_err());
    assert!(
        responsive,
        "filling a prior trash reservation must return while the journal lock is held"
    );
}

// One 2 s lock deadline per publication plus slack for a loaded runner.
const HELD_LOCK_BOUND: Duration = Duration::from_secs(10);

#[test]
fn a_lock_held_past_the_bound_returns_the_history_and_answers_the_next_write() {
    use std::os::unix::fs::PermissionsExt;
    let root = TestDir::new("publication-held-lock");
    let shared = root.dir("shared");
    root.assert_contains(&shared);
    std::fs::set_permissions(&shared, std::fs::Permissions::from_mode(0o700)).unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let mut ops = Ops::new(tx);
    ops.journal.attach_test_shared(shared.clone());
    let lock_path = shared.join("undo-journal.lock");
    root.assert_contains(&lock_path);
    let lock = std::fs::File::create(lock_path).unwrap();
    crate::uistore::lock_exclusive(&lock).unwrap();
    let first = root.file("first", "first");
    let mut out = Vec::new();
    publish(
        &mut ops,
        Publication::Push(Entry {
            op: "first".into(),
            steps: vec![Step::Created {
                path: first.clone(),
            }],
        }),
    );
    let returned = rx
        .recv_timeout(HELD_LOCK_BOUND)
        .expect("a lock held past the bound must not keep the history on the publisher");
    assert!(matches!(returned, OpMsg::JournalDone { .. }));
    super::super::opsdispatch::report_op(&mut out, &mut ops, returned);
    assert!(
        ops.journal.deferred.is_none() && ops.journal.has_pending(),
        "the history returns with its entry still pending, and its own return starts no publisher"
    );
    let source = root.file("before", "payload");
    root.assert_contains(&root.join("after"));
    let started = std::time::Instant::now();
    super::super::opsdispatch::do_rename(&mut out, &mut ops, source.to_str().unwrap(), "after");
    while !String::from_utf8_lossy(&out).contains("\"t\":\"renamed\"") {
        let left = HELD_LOCK_BOUND.saturating_sub(started.elapsed());
        let event = rx
            .recv_timeout(left)
            .expect("the next write answers while the lock stays held");
        super::super::opsdispatch::report_op(&mut out, &mut ops, event);
    }
    assert!(ops.journal.deferred.is_none() && ops.journal.has_pending());
    crate::uistore::unlock(&lock).unwrap();
    flush_pending(&mut ops);
    receive(&mut out, &mut ops, &rx);
    assert!(ops.journal.deferred.is_none() && !ops.journal.has_pending());
    root.assert_contains(&source);
    assert_eq!(ops.journal.undo().unwrap(), "rename");
    assert_eq!(std::fs::read_to_string(&source).unwrap(), "payload");
    root.assert_contains(&first);
    assert_eq!(ops.journal.undo().unwrap(), "first");
    assert!(ops.journal.undo().is_err());
}
