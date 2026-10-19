// Review follow-up coverage keeps every helper and file inside the marked test root.
use super::testdir::TestDir;
use super::undo::ItemIdentity;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc::channel, Arc};
use std::time::{Duration, Instant};

struct CancelOnDrop(Arc<AtomicBool>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

#[test]
fn review_action_retry_rechecks_each_item_before_the_process_runs() {
    let root = TestDir::new("jobs-action-retry-identity");
    let first = root.file("a.txt", "first failed file");
    let second = root.file("b.txt", "second failed file");
    let expected: Vec<_> = [&first, &second]
        .into_iter()
        .map(|path| (path.clone(), ItemIdentity::inspect(path).unwrap()))
        .collect();
    let script = root.file("probe.py", "import pathlib,sys,time\np=pathlib.Path(sys.argv[1])\nready,release,log=map(pathlib.Path,sys.argv[2:])\nif p.name=='a.txt':\n ready.write_text('ready')\n deadline=time.monotonic()+5\n while not release.exists() and time.monotonic()<deadline: time.sleep(0.01)\nwith log.open('a') as stream: stream.write(p.name+'\\n')\n");
    let ready = root.join("ready");
    let release = root.join("release");
    let log = root.join("calls");
    let manifest = super::actionmanifest::Manifest {
        name: "Inspect".into(),
        mime: vec!["text/*".into()],
        selection: "many".into(),
        mode: "background".into(),
        run: vec![
            "/usr/bin/python3".into(),
            script.to_str().unwrap().into(),
            "{path}".into(),
            ready.to_str().unwrap().into(),
            release.to_str().unwrap().into(),
            log.to_str().unwrap().into(),
        ],
    };
    let mut work = super::jobwork::Work::Action {
        action: super::actions::Action {
            id: "test".into(),
            manifest,
            script: false,
        },
        paths: vec![
            first.to_str().unwrap().into(),
            second.to_str().unwrap().into(),
        ],
        selection: None,
    };
    assert_eq!(work.retry(&expected).unwrap(), 2);
    let cancel = Arc::new(AtomicBool::new(false));
    let _stop = CancelOnDrop(Arc::clone(&cancel));
    let (tx, rx) = channel();
    work.start(1, cancel, tx);
    let end = Instant::now() + Duration::from_secs(5);
    while !ready.exists() {
        assert!(Instant::now() < end, "the first retry item did not start");
        std::thread::sleep(Duration::from_millis(5));
    }
    root.replace_file(&second, "unrelated replacement");
    root.file("release", "continue");
    loop {
        if let super::opsreq::OpMsg::JobDone { failed, error, .. } =
            rx.recv_timeout(Duration::from_secs(10)).unwrap()
        {
            assert!(!error.is_empty());
            assert_eq!(failed.len(), 1);
            assert_eq!(failed[0].0, second);
            assert!(
                failed[0].1.same_item(&expected[1].1),
                "failure must retain the original identity"
            );
            break;
        }
    }
    assert_eq!(
        std::fs::read_to_string(log).unwrap(),
        "a.txt\n",
        "a replacement must never reach the action"
    );
    assert_eq!(
        std::fs::read_to_string(second).unwrap(),
        "unrelated replacement"
    );
}

#[test]
fn review_manifest_controls_are_rejected_without_rejecting_supported_escapes() {
    let prefix = "name=\"Inspect\"\nmime=[\"text/*\"]\nselection=\"one\"\nmode=\"background\"\nrun=[\"/usr/bin/true\",\"";
    for byte in (0u8..=31)
        .chain(std::iter::once(127))
        .filter(|byte| *byte != b'\t')
    {
        assert!(
            super::actionmanifest::parse(&format!("{prefix}raw{}control\"]", byte as char))
                .is_err(),
            "raw control {byte}"
        );
    }
    for value in [
        r"escaped\nline",
        r"escaped\ttab",
        r"escaped\u0001control",
        "raw\ttab",
    ] {
        assert!(
            super::actionmanifest::parse(&format!("{prefix}{value}\"]")).is_ok(),
            "supported value {value:?}"
        );
    }
}

#[test]
fn review_job_token_expires_after_another_windows_undo_and_redo() {
    use super::opsdispatch::{report_op, start_transfer, Ops};
    let root = TestDir::new("jobs-token-replay");
    let shared = root.dir("journal");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&shared, std::fs::Permissions::from_mode(0o700)).unwrap();
    let source = root.file("source.txt", "owned data");
    let dest = root.dir("destination");
    let (tx, rx) = channel();
    let mut first = Ops::new(tx);
    first.journal.attach_test_shared(shared.clone());
    start_transfer(
        &mut Vec::new(),
        &mut first,
        "copy",
        vec![source.to_str().unwrap().into()],
        dest.to_str().unwrap(),
        Default::default(),
    );
    while first.jobs.unfinished() > 0 {
        report_op(
            &mut Vec::new(),
            &mut first,
            rx.recv_timeout(Duration::from_secs(10)).unwrap(),
        );
    }
    assert!(first.jobs.rows[0].undo);
    let mut second = super::undo::Journal::new();
    second.attach_test_shared(shared);
    second.undo().unwrap();
    second
        .redo(2, &AtomicBool::new(false), &channel().0)
        .unwrap();
    let mut out = Vec::new();
    super::jobevents::request(&mut out, &mut first, r#"{"op":"undo","id":1}"#);
    assert!(
        dest.join("source.txt").exists(),
        "a replay must not revive the old job's Undo offer"
    );
    assert!(!first.jobs.rows[0].undo);
    assert!(String::from_utf8(out)
        .unwrap()
        .contains("That job can no longer be undone."));
}

#[test]
fn review_job_claim_checks_the_entry_even_if_a_history_revision_is_reused() {
    use super::undo::{Entry, Journal, Step};
    use std::os::unix::fs::PermissionsExt;
    let root = TestDir::new("jobs-token-entry");
    let shared = root.dir("journal");
    std::fs::set_permissions(&shared, std::fs::Permissions::from_mode(0o700)).unwrap();
    let first = root.file("first.txt", "");
    let second = root.file("second.txt", "");
    let made = |path: &std::path::Path| Entry {
        op: "newfile".into(),
        steps: vec![Step::MadeFile {
            path: path.into(),
            identity: ItemIdentity::inspect(path).unwrap(),
        }],
    };
    let mut a = Journal::new();
    let mut b = Journal::new();
    a.attach_test_shared(shared.clone());
    b.attach_test_shared(shared.clone());
    let key = a.push_for_job(made(&first)).unwrap();
    b.push(made(&second));
    let file = shared.join(super::undoshare::JOURNAL_FILE);
    root.assert_contains(&file);
    let changed = std::fs::read_to_string(&file)
        .unwrap()
        .replace("\"jobGen\": 2", "\"jobGen\": 1");
    assert!(changed.contains("\"jobGen\": 1"));
    std::fs::write(&file, &changed).unwrap();
    assert!(a.undo_for_job(&key).is_err());
    assert!(
        first.exists() && second.exists(),
        "a token cannot claim another entry with the same revision"
    );
    assert_eq!(
        std::fs::read_to_string(file).unwrap(),
        changed,
        "an expired claim leaves the journal untouched"
    );
}

#[test]
fn review_archive_retry_keeps_the_failed_identity_after_refusing_a_replacement() {
    let root = TestDir::new("jobs-archive-retry-identity");
    let source = root.file("failed.txt", "original failed file");
    let expected = vec![(source.clone(), ItemIdentity::inspect(&source).unwrap())];
    let mut work = super::jobwork::Work::Archive {
        compressing: true,
        paths: vec![source.to_str().unwrap().into()],
        format: "zip".into(),
        archive: Default::default(),
        dest: root.join("out.zip"),
        formats: Arc::new(super::archive::Formats::from_tools(false, false)),
        selection: None,
        password: String::new(),
        encrypt_names: false,
    };
    work.retry(&expected).unwrap();
    root.replace_file(&source, "unrelated replacement");
    let (tx, rx) = channel();
    let mut ops = super::opsdispatch::Ops::new(tx.clone());
    let cancel = Arc::new(AtomicBool::new(false));
    work.start(1, Arc::clone(&cancel), tx);
    ops.jobs
        .adopt(&mut Vec::new(), 1, "compress", "", 1, cancel);
    ops.jobs.rows[0].work = Some(work);
    ops.jobs.rows[0].failed = expected.clone();
    let message = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    super::opsdispatch::report_op(&mut Vec::new(), &mut ops, message);
    let job = &mut ops.jobs.rows[0];
    assert!(job.state == super::jobs::State::Failed);
    assert!(job.reason.contains("Selected item changed"));
    assert_eq!(job.failed.len(), 1);
    assert!(job.failed[0].1.same_item(&expected[0].1));
    assert!(job.work.as_mut().unwrap().retry(&job.failed).is_err());
    assert_eq!(
        std::fs::read_to_string(source).unwrap(),
        "unrelated replacement"
    );
    assert!(!root.join("out.zip").exists());
}

#[test]
fn review_r2_archive_failure_keeps_identity_from_the_failed_invocation() {
    let root = TestDir::new("review-r2-archive-first-failure");
    let source = root.file("failed.txt", "original failed file");
    let original = ItemIdentity::inspect(&source).unwrap();
    let mut work = super::jobwork::Work::Archive {
        compressing: true,
        paths: vec![source.to_str().unwrap().into()],
        format: "zip".into(),
        archive: Default::default(),
        dest: root.join("out.zip"),
        formats: Arc::new(super::archive::Formats::from_tools(false, false)),
        selection: None,
        password: String::new(),
        encrypt_names: false,
    };
    let (tx, rx) = channel();
    let mut ops = super::opsdispatch::Ops::new(tx.clone());
    let cancel = Arc::new(AtomicBool::new(false));
    work.start(1, Arc::clone(&cancel), tx);
    ops.jobs
        .adopt(&mut Vec::new(), 1, "compress", "", 1, cancel);
    ops.jobs.rows[0].work = Some(work);
    let message = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    root.replace_file(&source, "unrelated replacement");
    super::opsdispatch::report_op(&mut Vec::new(), &mut ops, message);
    let job = &mut ops.jobs.rows[0];
    assert!(job.state == super::jobs::State::Failed);
    let kept = job
        .failed
        .first()
        .is_some_and(|(_, id)| id.same_item(&original));
    let accepted = job.work.as_mut().unwrap().retry(&job.failed).is_ok();
    eprintln!("REPRO initial archive failure: original identity retained={kept}, replacement accepted for Retry={accepted}");
    assert!(
        kept && !accepted,
        "Retry accepted a replacement which never failed"
    );
}

#[test]
fn review_r2_overlapping_undos_keep_both_redo_entries() {
    use super::undo::{Entry, Step};
    use std::os::unix::fs::PermissionsExt;
    let root = TestDir::new("review-r2-concurrent-redo");
    let shared_dir = root.dir("journal");
    std::fs::set_permissions(&shared_dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut journal = super::undo::Journal::new();
    journal.attach_test_shared(shared_dir.clone());
    for name in ["a", "b"] {
        let before = root.join(&format!("{name}-old.txt"));
        let after = root.file(&format!("{name}-new.txt"), name);
        let identity = ItemIdentity::inspect(&after).unwrap();
        journal.push(Entry {
            op: "rename".into(),
            steps: vec![Step::Moved {
                from: before,
                to: after,
                before: identity.clone(),
                after: identity,
                rename_fat: false,
            }],
        });
    }
    let shared = super::undoshare::Shared::at(shared_dir.clone()).unwrap();
    let (newer, newer_gen) = super::undoshare::claim_undo(&shared).unwrap().unwrap();
    let (older, older_gen) = super::undoshare::claim_undo(&shared).unwrap().unwrap();
    for entry in [&newer, &older] {
        let Step::Moved { from, to, .. } = &entry.steps[0] else {
            panic!("moved");
        };
        root.assert_contains(from);
        root.assert_contains(to);
        std::fs::rename(to, from).unwrap();
    }
    super::undoshare::finish_undone(&shared, Some(older), &[], older_gen).unwrap();
    super::undoshare::finish_undone(&shared, Some(newer), &[], newer_gen).unwrap();
    let text = std::fs::read_to_string(shared_dir.join(super::undoshare::JOURNAL_FILE)).unwrap();
    let doc = super::undocodec::decode(&text).unwrap();
    eprintln!(
        "REPRO overlapping Undo: two reversals, retained Redo entries={}",
        doc.redo.len()
    );
    assert_eq!(
        doc.redo.len(),
        2,
        "another Undo is not a new operation that invalidates Redo"
    );
}
