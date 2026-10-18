use super::*;
use crate::backend::testdir::TestDir;

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
