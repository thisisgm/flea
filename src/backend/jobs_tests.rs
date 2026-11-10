// Pure model checks complement the real-backend suite without touching the filesystem.
#[path = "jobs_queued_paste_tests.rs"]
mod queued_paste;

use super::jobs::{Jobs, State};
use super::opscancel::DetachedJobs;
use super::opsdispatch::{report_op, Ops};
use super::opsreq::OpMsg;
use super::undo::{Entry, Step};
use std::sync::atomic::AtomicBool;
use std::sync::{mpsc::channel, Arc};

#[test]
fn jobs_late_journal_completion_revokes_the_previous_jobs_undo() {
    let (tx, _) = channel();
    let mut ops = Ops::new(tx);
    let mut out = Vec::new();
    ops.jobs
        .adopt(&mut out, 1, "copy", "", 1, Arc::new(AtomicBool::new(false)));
    report_op(
        &mut out,
        &mut ops,
        OpMsg::TransferDone {
            id: 1,
            ok: 1,
            failed: 0,
            skipped: 0,
            cancelled: false,
            entry: Entry {
                op: "copy".into(),
                steps: vec![Step::Barrier],
            },
            retry: Vec::new(),
            durable: false,
            note: String::new(),
            refused: Vec::new(),
        },
    );
    assert!(ops.jobs.rows[0].undo);
    report_op(
        &mut out,
        &mut ops,
        OpMsg::MkdirDone {
            id: 2,
            result: Ok((std::path::PathBuf::new(), vec![Step::Barrier])),
        },
    );
    assert!(
        !ops.jobs.rows[0].undo,
        "Undo must never target the later journal entry"
    );
}

#[test]
fn jobs_late_rename_publication_revokes_the_previous_jobs_undo() {
    let root = super::testdir::TestDir::new("jobs-late-rename");
    let path = root.file("before", "payload");
    let (tx, rx) = channel();
    let mut ops = Ops::new(tx);
    let mut out = Vec::new();
    ops.jobs
        .adopt(&mut out, 1, "copy", "", 1, Arc::new(AtomicBool::new(false)));
    ops.jobs.rows[0].state = State::Done;
    ops.jobs.rows[0].undo = true;
    ops.jobs.rows[0].undo_key = ops.journal.push_for_job(Entry {
        op: "copy".into(),
        steps: vec![Step::Barrier],
    });
    let renamed = path.clone();
    super::renamework::start(
        &mut ops,
        path.to_string_lossy().into(),
        std::time::Duration::from_secs(10),
        move || Ok((renamed, vec![Step::Barrier])),
    );
    let done = rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
    assert!(matches!(done, OpMsg::RenameDone { .. }));
    report_op(&mut out, &mut ops, done);
    assert!(
        ops.journal.deferred.is_some() && ops.jobs.rows[0].undo,
        "the offer stands while the rename's entry is still with the publisher"
    );
    let published = rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
    assert!(matches!(published, OpMsg::JournalDone { .. }));
    report_op(&mut out, &mut ops, published);
    assert!(
        !ops.jobs.rows[0].undo,
        "a late rename's publication must revoke the earlier job's Undo"
    );
}

#[test]
fn jobs_retry_waits_for_a_younger_running_job_on_the_same_drive() {
    let (tx, _) = channel();
    let mut jobs = Jobs::default();
    let mut out = Vec::new();
    for id in [1, 2] {
        jobs.adopt(
            &mut out,
            id,
            "copy",
            "",
            1,
            Arc::new(AtomicBool::new(false)),
        );
        jobs.rows.last_mut().unwrap().drive = Some("drive".into());
        jobs.rows.last_mut().unwrap().target = "NAS".into();
    }
    jobs.rows[0].state = State::Waiting;
    jobs.rows[0].order = 3;
    jobs.advance(&mut out, &DetachedJobs::new(), &tx);
    assert!(jobs.rows[0].state == State::Waiting);
    assert_eq!(jobs.rows[0].reason, "waiting for the copy to NAS");
    jobs.rows[1].state = State::Done;
    jobs.advance(&mut out, &DetachedJobs::new(), &tx);
    assert!(jobs.rows[0].state == State::Running);
    assert_eq!(jobs.unfinished(), 1);
}

fn review_drain(ops: &mut Ops, rx: &std::sync::mpsc::Receiver<OpMsg>) {
    let mut out = Vec::new();
    while ops.jobs.unfinished() > 0 {
        let msg = rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("worker reply");
        report_op(&mut out, ops, msg);
    }
}

#[test]
fn review_retry_must_not_move_a_replacement_created_while_queued() {
    use super::testdir::TestDir;
    use std::os::unix::fs::PermissionsExt;
    let root = TestDir::new("review-retry-queued");
    let source = root.file("source.txt", "original failed source");
    let dest = root.dir("destination");
    let taken = root.file("destination/source.txt", "existing target");
    std::fs::set_permissions(&taken, std::fs::Permissions::from_mode(0o400)).unwrap();
    let (tx, rx) = channel();
    let mut ops = Ops::new(tx);
    let mut out = Vec::new();
    super::opsdispatch::start_transfer(
        &mut out,
        &mut ops,
        "move",
        vec![source.to_string_lossy().into()],
        dest.to_str().unwrap(),
        Default::default(),
    );
    review_drain(&mut ops, &rx);
    assert!(ops.jobs.rows[0].state == State::Failed);
    assert_eq!(ops.jobs.rows[0].failed.len(), 1);
    root.assert_contains(&taken);
    std::fs::remove_file(&taken).unwrap();
    let drive = ops.jobs.rows[0].drive.clone();
    ops.jobs.adopt(
        &mut out,
        2,
        "compress",
        "",
        1,
        Arc::new(AtomicBool::new(false)),
    );
    ops.jobs.rows[1].drive = drive;
    ops.jobs.rows[1].target = "NAS".into();
    super::jobevents::request(&mut out, &mut ops, r#"{"op":"retry","id":1}"#);
    assert!(ops.jobs.rows[0].state == State::Waiting);
    root.replace_file(&source, "unrelated replacement");
    ops.jobs.rows[1].state = State::Done;
    ops.jobs.advance(&mut out, &ops.detached, &ops.tx);
    review_drain(&mut ops, &rx);
    eprintln!(
        "REPRO queued retry: replacement source exists={}, destination={:?}",
        source.exists(),
        std::fs::read_to_string(dest.join("source.txt"))
    );
    assert!(
        source.exists(),
        "queued Retry moved the replacement instead of refusing it"
    );
}

#[test]
fn review_job_undo_must_not_reverse_another_windows_job() {
    use super::testdir::TestDir;
    use std::os::unix::fs::PermissionsExt;
    let root = TestDir::new("review-shared-job-undo");
    let shared = root.dir("journal");
    std::fs::set_permissions(&shared, std::fs::Permissions::from_mode(0o700)).unwrap();
    let source_a = root.file("a.txt", "window A");
    let source_b = root.file("b.txt", "window B");
    let dest_a = root.dir("dest-a");
    let dest_b = root.dir("dest-b");
    let (tx_a, rx_a) = channel();
    let (tx_b, rx_b) = channel();
    let mut a = Ops::new(tx_a);
    let mut b = Ops::new(tx_b);
    a.journal.attach_test_shared(shared.clone());
    b.journal.attach_test_shared(shared);
    let mut out = Vec::new();
    super::opsdispatch::start_transfer(
        &mut out,
        &mut a,
        "copy",
        vec![source_a.to_string_lossy().into()],
        dest_a.to_str().unwrap(),
        Default::default(),
    );
    review_drain(&mut a, &rx_a);
    assert!(a.jobs.rows[0].undo);
    super::opsdispatch::start_transfer(
        &mut out,
        &mut b,
        "copy",
        vec![source_b.to_string_lossy().into()],
        dest_b.to_str().unwrap(),
        Default::default(),
    );
    review_drain(&mut b, &rx_b);
    out.clear();
    super::jobevents::request(&mut out, &mut a, r#"{"op":"undo","id":1}"#);
    assert!(String::from_utf8_lossy(&out).contains("That job can no longer be undone."));
    assert!(!a.jobs.rows[0].undo);
    assert_eq!(
        std::fs::read_to_string(dest_a.join("a.txt")).unwrap(),
        "window A"
    );
    eprintln!(
        "REPRO shared Undo: window A copy exists={}, window B copy exists={}",
        dest_a.join("a.txt").exists(),
        dest_b.join("b.txt").exists()
    );
    assert!(
        dest_b.join("b.txt").exists(),
        "Undo for window A removed window B's copy"
    );
}

#[test]
fn hostile_argv_names_remain_literal_and_option_safe() {
    let manifest = super::actionmanifest::parse(
        r#"name = "Inspect"
mime = ["text/*"]
selection = "many"
run = ["program", "{path}", "{dir}", "{name}", "{stem}", "{ext}"]
mode = "background"
"#,
    )
    .unwrap();
    let action = super::actions::Action {
        id: "test".into(),
        manifest,
        script: false,
    };
    let path =
        std::path::Path::new("/owned/- 'quote' \"double\"\n$(touch SHOULD_NOT_RUN) {ext}.txt");
    let argv = action.argv(path);
    assert_eq!(argv[0], "program");
    assert_eq!(argv[1], path.to_str().unwrap());
    assert_eq!(argv[2], "/owned");
    assert_eq!(
        argv[3],
        format!("./{}", path.file_name().unwrap().to_str().unwrap())
    );
    assert_eq!(
        argv[4],
        format!("./{}", path.file_stem().unwrap().to_str().unwrap())
    );
    assert_eq!(argv[5], "txt");
}

#[test]
fn review_manifest_rejects_unescaped_control_bytes() {
    let manifest = "name = \"Inspect\"\nmime = [\"text/*\"]\nselection = \"one\"\nrun = [\"/usr/bin/true\", \"raw\u{1}control\"]\nmode = \"background\"\n";
    let parsed = super::actionmanifest::parse(manifest);
    eprintln!(
        "REPRO raw 0x01 in manifest argument accepted={}",
        parsed.is_ok()
    );
    assert!(
        parsed.is_err(),
        "a malformed TOML basic string was accepted"
    );
}

#[test]
fn legacy_cancel_finishes_a_queued_extract_without_cancelling_its_predecessor() {
    use super::archive::Formats;
    use super::jobwork::Work;
    use std::sync::atomic::Ordering;
    let (tx, _) = channel();
    let mut ops = Ops::new(tx);
    let mut out = Vec::new();
    let running = Arc::new(AtomicBool::new(false));
    let waiting = Arc::new(AtomicBool::new(false));
    for (id, flag) in [(11, &running), (12, &waiting)] {
        ops.detached.insert(id, flag);
        ops.jobs
            .adopt(&mut out, id, "extract", "Extracting", 0, Arc::clone(flag));
        let job = ops.jobs.rows.last_mut().unwrap();
        job.drive = Some("same-drive".into());
        job.target = "target".into();
    }
    let root = super::testdir::TestDir::new("jobs-queued-extract-cancel");
    let queued = &mut ops.jobs.rows[1];
    queued.state = State::Waiting;
    queued.work = Some(Work::Archive {
        compressing: false,
        paths: Vec::new(),
        format: String::new(),
        archive: root.join("not-opened.tar"),
        dest: root.join("not-created"),
        formats: Arc::new(Formats::from_tools(false, false)),
        selection: None,
        password: String::new(),
        encrypt_names: false,
    });
    out.clear();
    super::opsdispatch::cancel_transfer(&mut out, &mut ops, 12);
    assert!(ops.jobs.rows[1].state == State::Done);
    assert!(ops.jobs.rows[1].cancelled);
    assert!(ops.jobs.rows[0].state == State::Running);
    assert!(!running.load(Ordering::Relaxed));
    assert!(String::from_utf8(out)
        .unwrap()
        .contains(r#"{"t":"archivedone","id":12,"ok":false,"verified":true,"err":"cancelled"}"#));
}
