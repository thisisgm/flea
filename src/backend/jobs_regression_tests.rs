use super::jobs::State;
use super::opsdispatch::{report_op, Ops};
use super::opsreq::OpMsg;
use super::testdir::TestDir;
use super::undo::{Entry, Step};
use std::sync::atomic::AtomicBool;
use std::sync::{mpsc::channel, Arc};

#[test]
fn manifest_name_accepts_documented_tabs_and_escapes() {
    for (name, parsed) in [
        ("A\tB", "A\tB"),
        ("A\\tB", "A\tB"),
        ("A\\nB", "A\nB"),
        ("A\\u0007B", "A\u{7}B"),
    ] {
        let text = format!(
            "name=\"{name}\"\nmime=[\"text/*\"]\nselection=\"one\"\nmode=\"background\"\nrun=[\"/usr/bin/true\"]\n"
        );
        let manifest = super::actionmanifest::parse(&text)
            .unwrap_or_else(|error| panic!("documented name {name:?} rejected: {error}"));
        assert_eq!(manifest.name, parsed);
    }
}

#[test]
fn root_action_completes_without_panic() {
    let action = super::actions::Action {
        id: "script:test".into(),
        script: true,
        manifest: super::actionmanifest::Manifest {
            name: "test".into(),
            mime: vec!["*/*".into()],
            selection: "many".into(),
            mode: "background".into(),
            run: vec!["/usr/bin/true".into()],
        },
    };
    let (tx, rx) = channel();
    let result = std::panic::catch_unwind(|| {
        super::actionrun::background(
            1,
            &action,
            vec!["/".into()],
            None,
            Arc::new(AtomicBool::new(false)),
            tx,
        )
    });
    assert!(result.is_ok(), "root action panics before JobDone");
    assert!(rx
        .try_iter()
        .any(|message| matches!(message, OpMsg::JobDone { .. })));
}

#[test]
fn failed_target_preparation_releases_unrelated_drive() {
    let mut ops = Ops::new(channel().0);
    let jobs = &mut ops.jobs;
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
    }
    for job in &mut jobs.rows {
        job.state = State::Waiting;
    }
    jobs.rows[0].prepared = false;
    jobs.rows[0].target = "stalled".into();
    jobs.rows[1].drive = Some("healthy-local".into());
    super::opsdispatch::report_op(
        &mut out,
        &mut ops,
        OpMsg::JobReady {
            id: 1,
            result: Err(crate::error::FleaError {
                where_: "jobs".into(),
                path: "/stalled".into(),
                msg: "target is not responding".into(),
            }),
        },
    );
    assert!(ops.jobs.rows[0].state == State::Failed);
    assert!(ops.jobs.rows[1].state == State::Running);
    assert_eq!(ops.jobs.unfinished(), 1);
    assert!(!String::from_utf8_lossy(&out).contains("waiting for the copy to stalled"));
}

fn stalled() -> crate::error::FleaError {
    crate::error::FleaError {
        where_: "jobs".into(),
        path: "/stalled".into(),
        msg: "target is not responding".into(),
    }
}

#[test]
fn failed_target_preparation_answers_the_legacy_terminal_line() {
    let transfer = super::jobwork::Work::Transfer {
        moving: false,
        paths: vec!["/stalled-a".into(), "/stalled-b".into()],
        dest: "/stalled".into(),
        selection: None,
        destination: None,
        policy: Default::default(),
    };
    let archive = super::jobwork::Work::Archive {
        compressing: false,
        paths: Vec::new(),
        format: String::new(),
        archive: "/stalled.tar".into(),
        dest: "/stalled".into(),
        formats: Arc::new(super::archive::Formats::test_stub(Vec::new(), Vec::new())),
        selection: None,
        password: String::new(),
        encrypt_names: false,
    };
    for (work, line) in [
        (transfer, r#"{"t":"transferdone","id":1,"ok":0,"failed":2,"skipped":0,"cancelled":false,"retryPaths":[],"durable":false,"note":"target is not responding"}"#),
        (archive, r#"{"t":"archivedone","id":1,"ok":false,"verified":true,"err":"target is not responding"}"#),
    ] {
        let mut ops = Ops::new(channel().0);
        let mut out = Vec::new();
        ops.jobs
            .adopt(&mut out, 1, "copy", "", 2, Arc::new(AtomicBool::new(false)));
        let job = &mut ops.jobs.rows[0];
        job.state = State::Waiting;
        job.prepared = false;
        job.work = Some(work);
        out.clear();
        report_op(&mut out, &mut ops, OpMsg::JobReady { id: 1, result: Err(stalled()) });
        assert!(ops.jobs.rows[0].state == State::Failed);
        let text = String::from_utf8_lossy(&out);
        assert!(text.lines().any(|l| l == line), "{line} missing from {text}");
    }
}

#[test]
fn a_job_whose_target_check_failed_retries_whole_and_checks_again() {
    let root = TestDir::new("jobs-target-retry");
    let source = root.file("source", "payload");
    let dest = root.dir("dest");
    root.assert_contains(&dest.join("source"));
    let (tx, rx) = channel();
    let mut ops = Ops::new(tx);
    let mut out = Vec::new();
    ops.jobs.submit(
        &mut out,
        1,
        super::jobwork::Spec { kind: "copy", label: "", target: Some(dest.clone()) },
        super::jobwork::Work::Transfer {
            moving: false,
            paths: vec![source.to_string_lossy().into()],
            dest: dest.clone(),
            selection: None,
            destination: None,
            policy: Default::default(),
        },
        &ops.detached,
        &ops.tx,
    );
    let wait = std::time::Duration::from_secs(10);
    assert!(matches!(rx.recv_timeout(wait).unwrap(), OpMsg::JobReady { result: Ok(_), .. }));
    report_op(&mut out, &mut ops, OpMsg::JobReady { id: 1, result: Err(stalled()) });
    assert!(ops.jobs.rows[0].state == State::Failed && !dest.join("source").exists());
    out.clear();
    super::jobevents::request(&mut out, &mut ops, r#"{"op":"retry","id":1}"#);
    assert!(
        !String::from_utf8_lossy(&out).contains("joberror"),
        "a job that never started must be retryable: {}",
        String::from_utf8_lossy(&out)
    );
    while ops.jobs.unfinished() > 0 {
        report_op(&mut out, &mut ops, rx.recv_timeout(wait).expect("the retried job checks its target again and runs"));
    }
    assert!(ops.jobs.rows[0].state == State::Done, "{}", String::from_utf8_lossy(&out));
    assert_eq!(std::fs::read_to_string(dest.join("source")).unwrap(), "payload");
}

// A cancel during the target check ends the job, so no retry can put a second check beside the first.
#[test]
fn a_job_cancelled_during_its_target_check_is_not_retried() {
    let root = TestDir::new("jobs-target-cancel");
    let source = root.file("source", "payload");
    let dest = root.dir("dest");
    root.assert_contains(&dest.join("source"));
    let (tx, rx) = channel();
    let mut ops = Ops::new(tx);
    let mut out = Vec::new();
    ops.jobs.submit(
        &mut out,
        1,
        super::jobwork::Spec { kind: "copy", label: "", target: Some(dest.clone()) },
        super::jobwork::Work::Transfer {
            moving: false,
            paths: vec![source.to_string_lossy().into()],
            dest: dest.clone(),
            selection: None,
            destination: None,
            policy: Default::default(),
        },
        &ops.detached,
        &ops.tx,
    );
    assert!(ops.jobs.rows[0].state == State::Waiting && !ops.jobs.rows[0].prepared);
    super::jobevents::request(&mut out, &mut ops, r#"{"op":"cancel","id":1}"#);
    assert!(ops.jobs.rows[0].state == State::Done && ops.jobs.rows[0].cancelled);
    out.clear();
    super::jobevents::request(&mut out, &mut ops, r#"{"op":"retry","id":1}"#);
    let text = String::from_utf8_lossy(&out).into_owned();
    assert!(text.contains(r#"{"t":"joberror","id":1,"error":"Only a failed job can be retried."}"#), "{text}");
    let held = rx.recv_timeout(std::time::Duration::from_secs(10)).expect("the first check answers");
    assert!(matches!(held, OpMsg::JobReady { id: 1, .. }));
    report_op(&mut out, &mut ops, held);
    assert!(ops.jobs.rows[0].state == State::Done && ops.jobs.rows[0].cancelled);
    assert!(!dest.join("source").exists());
    assert!(rx.recv_timeout(std::time::Duration::from_millis(500)).is_err(), "only one check ever ran");
}

#[test]
fn retry_clears_attempt_fields() {
    let sandbox = TestDir::new("jobs-retry");
    let root = sandbox.path().to_path_buf();
    let source = root.join("source");
    std::fs::write(&source, "source").unwrap();
    let identity = super::undo::ItemIdentity::inspect(&source).unwrap();
    let (tx, _) = channel();
    let mut ops = Ops::new(tx);
    let mut out = Vec::new();
    for id in [1, 2] {
        ops.jobs.adopt(
            &mut out,
            id,
            "copy",
            "",
            1,
            Arc::new(AtomicBool::new(false)),
        );
    }
    for job in &mut ops.jobs.rows {
        job.drive = Some("same".into());
    }
    let job = &mut ops.jobs.rows[0];
    job.state = State::Failed;
    job.cancelled = true;
    job.item_bytes = 999;
    job.failed = vec![(source.clone(), identity)];
    job.work = Some(super::jobwork::Work::Transfer {
        moving: false,
        paths: vec![source.to_str().unwrap().into()],
        dest: root.join("dest"),
        selection: None,
        destination: None,
        policy: Default::default(),
    });
    super::jobevents::request(&mut out, &mut ops, r#"{"op":"retry","id":1}"#);
    let job = &ops.jobs.rows[0];
    assert!(job.state == State::Waiting);
    assert!(!job.cancelled && job.item_bytes == 0);
}

#[test]
fn menu_rename_obeys_jobs_busy_gate() {
    let sandbox = TestDir::new("jobs-menu");
    let root = sandbox.path().to_path_buf();
    let path = root.join("before.txt");
    std::fs::write(&path, "original").unwrap();
    let (tx, rx) = channel();
    let mut ops = Ops::new(tx.clone());
    let menu = super::menu_actions::MenuActions::new(tx);
    assert!(menu.request(
        r#"{"c":"menuaction","op":"snapshot","id":7}"#.into(),
        vec![path.to_str().unwrap().into()],
        None
    ));
    assert!(matches!(
        rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap(),
        OpMsg::Meta { .. }
    ));
    ops.menuactions = Some(menu);
    let mut out = Vec::new();
    ops.jobs
        .adopt(&mut out, 1, "copy", "", 1, Arc::new(AtomicBool::new(false)));
    let result = super::opsdispatch::do_menu_rename(
        &mut out,
        &mut ops,
        path.to_str().unwrap(),
        "after.txt",
        7,
    );
    assert!(result.is_empty());
    let accepted = !ops.pending.is_empty();
    while !ops.pending.is_empty() || ops.journal.deferred.is_some() {
        let msg = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        report_op(&mut out, &mut ops, msg);
    }
    assert!(!accepted, "menu rename accepted while F2 would refuse");
    assert!(path.exists() && !root.join("after.txt").exists());
    assert!(String::from_utf8_lossy(&out).contains(r#""where":"rename""#));
}

fn publication_entry(path: &std::path::Path) -> Entry {
    Entry {
        op: "copy".into(),
        steps: vec![Step::MadeFile {
            path: path.into(),
            identity: super::undo::ItemIdentity::inspect(path).unwrap(),
        }],
    }
}

fn drain_publications(out: &mut Vec<u8>, ops: &mut Ops, rx: &std::sync::mpsc::Receiver<OpMsg>) {
    while ops.journal.deferred.is_some() {
        report_op(
            out,
            ops,
            rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap(),
        );
    }
}

fn completed_copy(id: usize, entry: Entry) -> OpMsg {
    OpMsg::TransferDone {
        id,
        ok: 1,
        failed: 0,
        skipped: 0,
        cancelled: false,
        entry,
        retry: Vec::new(),
        durable: false,
        note: String::new(),
        refused: Vec::new(),
    }
}

#[test]
fn a_deferred_job_entry_files_after_earlier_publications_without_a_token() {
    let root = TestDir::new("job-publication");
    let earlier = root.file("earlier", "");
    let later = root.file("later", "");
    let (tx, rx) = channel();
    let mut ops = Ops::new(tx);
    let mut out = Vec::new();
    ops.jobs
        .adopt(&mut out, 1, "copy", "", 1, Arc::new(AtomicBool::new(false)));
    ops.journal.deferred = Some(Vec::new());
    ops.journal.push(publication_entry(&earlier));
    report_op(
        &mut out,
        &mut ops,
        completed_copy(1, publication_entry(&later)),
    );
    assert!(ops.jobs.rows[0].state == State::Done && !ops.jobs.rows[0].undo);
    super::renamework::published(&mut out, &mut ops, super::undo::Journal::new(), Vec::new());
    drain_publications(&mut out, &mut ops, &rx);
    assert_eq!(ops.journal.len(), 2);
    root.assert_contains(&earlier);
    root.assert_contains(&later);
    assert_eq!(ops.journal.undo().unwrap(), "copy");
    assert!(earlier.exists() && !later.exists());
    assert_eq!(ops.journal.undo().unwrap(), "copy");
    assert!(!earlier.exists());
}

#[test]
fn an_undo_check_never_starts_an_unused_journal() {
    let root = TestDir::new("undo-check-lazy");
    let made = root.file("made", "");
    let journal = super::lazy::Local::<super::undo::Journal>::new(|| panic!("the undo check started the journal"));
    let mut out = Vec::new();
    let mut jobs = super::jobs::Jobs::default();
    jobs.adopt(&mut out, 1, "copy", "", 1, Arc::new(AtomicBool::new(false)));
    jobs.rows[0].undo = true;
    jobs.rows[0].undo_key = super::undo::Journal::new().push_for_job(publication_entry(&made));
    assert!(jobs.rows[0].undo_key.is_some());
    jobs.refresh_undo(&mut out, &journal);
    assert!(journal.get().is_none() && jobs.rows[0].undo);
}
