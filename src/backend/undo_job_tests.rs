use super::*;
use crate::backend::testdir::TestDir;
use std::os::unix::fs::PermissionsExt;

fn creation(path: &Path) -> Entry {
    Entry {
        op: "newfile".into(),
        steps: vec![Step::MadeFile {
            path: path.into(),
            identity: ItemIdentity::inspect(path).unwrap(),
        }],
    }
}

fn shared_dir(root: &TestDir) -> PathBuf {
    let path = root.dir("journal");
    root.assert_contains(&path);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    path
}

#[test]
fn local_job_token_expires_after_an_identical_entry_is_pushed() {
    let root = TestDir::new("job-token-push");
    let path = root.file("created", "");
    let entry = creation(&path);
    let mut journal = Journal::new();
    let key = journal.push_for_job(entry.clone()).unwrap();
    journal.push(entry);
    root.assert_contains(&path);
    assert_eq!(
        journal.undo_for_job(&key).unwrap_err().msg,
        "That job can no longer be undone."
    );
    assert!(path.exists());
    assert_eq!(journal.len(), 2);
}

#[test]
fn local_job_token_expires_after_plain_undo_reveals_its_entry_again() {
    let root = TestDir::new("job-token-undo");
    let first = root.file("first", "");
    let later = root.file("later", "");
    let mut journal = Journal::new();
    let key = journal.push_for_job(creation(&first)).unwrap();
    journal.push(creation(&later));
    root.assert_contains(&later);
    journal.undo().unwrap();
    root.assert_contains(&first);
    assert_eq!(
        journal.undo_for_job(&key).unwrap_err().msg,
        "That job can no longer be undone."
    );
    assert!(first.exists() && !later.exists());
    assert_eq!(journal.len(), 1);
}

#[test]
fn contended_job_undo_keeps_shared_history_and_can_retry() {
    let root = TestDir::new("job-undo-contention");
    let path = root.file("created", "");
    let shared = shared_dir(&root);
    let mut journal = Journal::new();
    journal.attach_test_shared(shared.clone());
    let key = journal.push_for_job(creation(&path)).unwrap();
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(shared.join("undo-journal.lock"))
        .unwrap();
    crate::uistore::lock_exclusive(&lock).unwrap();
    assert!(journal.undo_for_job(&key).unwrap_err().msg.contains("busy"));
    assert!(journal.shared.is_some() && path.exists());
    drop(lock);
    root.assert_contains(&path);
    journal.undo_for_job(&key).unwrap();
    assert!(!path.exists() && journal.shared.is_some());
}

#[test]
fn contended_push_stays_shared_and_retries_without_losing_order() {
    let root = TestDir::new("job-push-contention");
    let first = root.file("first", "");
    let later = root.file("later", "");
    let shared = shared_dir(&root);
    let mut a = Journal::new();
    let mut b = Journal::new();
    a.attach_test_shared(shared.clone());
    b.attach_test_shared(shared.clone());
    let second = root.file("second", "");
    let lock = std::fs::File::create(shared.join("undo-journal.lock")).unwrap();
    crate::uistore::lock_exclusive(&lock).unwrap();
    a.push(creation(&first));
    assert!(a.shared.is_some() && a.has_pending());
    drop(lock);
    a.push(creation(&second));
    assert_eq!(a.pending.len(), 2, "a push behind a queued entry waits even once the lock is free");
    a.retry_pending();
    assert!(!a.has_pending() && a.shared.is_some());
    b.push(creation(&later));
    root.assert_contains(&first);
    root.assert_contains(&second);
    root.assert_contains(&later);
    a.undo().unwrap();
    assert!(first.exists() && second.exists() && !later.exists());
    a.undo().unwrap();
    assert!(first.exists() && !second.exists());
    a.undo().unwrap();
    assert!(!first.exists());
}

#[test]
fn contended_redo_keeps_shared_history_and_only_a_failed_open_detaches() {
    let root = TestDir::new("redo-contention");
    let from = root.file("a.txt", "payload");
    let to = root.join("b.txt");
    root.assert_contains(&to);
    let shared = shared_dir(&root);
    let mut journal = Journal::new();
    journal.attach_test_shared(shared.clone());
    let (_, steps) = crate::backend::ops::rename(&from, "b.txt").unwrap();
    journal.push(Entry {
        op: "rename".into(),
        steps,
    });
    assert_eq!(journal.undo().unwrap(), "rename");
    assert!(from.exists() && !to.exists());
    let lock_path = shared.join("undo-journal.lock");
    root.assert_contains(&lock_path);
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&lock_path)
        .unwrap();
    crate::uistore::lock_exclusive(&lock).unwrap();
    let (tx, _rx) = std::sync::mpsc::channel();
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let busy = journal.redo(1, &cancel, &tx).unwrap_err();
    assert_eq!(busy.msg, "the undo history is busy; try again");
    assert!(journal.shared.is_some() && from.exists() && !to.exists());
    let handle = crate::backend::undoshare::Shared::at(shared.clone()).unwrap();
    assert!(crate::backend::undoshare::claim_redo(&handle).is_err() && handle.contended());
    drop(lock);
    assert_eq!(journal.redo(2, &cancel, &tx).unwrap(), "rename");
    assert!(!from.exists() && to.exists() && journal.shared.is_some());
    assert!(lock_path.is_absolute() && !lock_path.as_os_str().is_empty());
    std::fs::remove_file(&lock_path).unwrap();
    std::fs::create_dir(&lock_path).unwrap();
    assert!(crate::backend::undoshare::claim_redo(&handle).is_err() && !handle.contended());
    let failed = journal.redo(3, &cancel, &tx).unwrap_err();
    assert_eq!(failed.msg, "the undo history could not be read");
    assert!(journal.shared.is_none());
}
