use super::*;
use crate::backend::testdir::TestDir;
use std::collections::HashMap;

#[test]
fn bfx264_completed_verbose_member_does_not_hide_the_growing_member() {
    live_member_samples(false);
    live_member_samples(true);
}

fn live_member_samples(protected: bool) {
    let root = TestDir::new("extract-live-members");
    root.file("first", "abc");
    let growing = root.file("large", "1234");
    let index = Index {
        known: true,
        total: 2,
        bytes: 15,
        members: HashMap::from([("first".into(), 3), ("large".into(), 12)]),
        ..Index::default()
    };
    let mut progress = Progress::new(index, protected);
    let (tx, rx) = std::sync::mpsc::channel();
    let mut ops = crate::backend::opsdispatch::Ops::new(tx.clone());
    ops.jobs.adopt(
        &mut Vec::new(),
        264,
        "extract",
        "Extracting",
        0,
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );
    progress.line("- first", true, root.path());
    for (size, expected) in [(4, 7), (8, 11)] {
        std::fs::write(&growing, vec![b'x'; size]).unwrap();
        progress.reported = Instant::now() - PROGRESS_EVERY;
        progress.sample(root.path(), 264, &tx, false);
        let sample = rx.try_recv().expect("live extraction progress");
        match &sample {
            OpMsg::JobEntries {
                done,
                total,
                bytes,
                total_bytes,
                file,
                ..
            } => {
                eprintln!(
                    "BFX264 sample done={done}/{total} bytes={bytes}/{total_bytes} file={file}"
                );
                assert_eq!((*done, *total, *bytes, *total_bytes), (1, 2, expected, 15));
                assert_eq!(file, "large");
            }
            _ => panic!("expected extraction sample"),
        }
        let job = &mut ops.jobs.rows[0];
        job.reported = Instant::now() - PROGRESS_EVERY;
        job.started = Instant::now() - std::time::Duration::from_secs(2);
        let mut wire = Vec::new();
        crate::backend::opsdispatch::report_op(&mut wire, &mut ops, sample);
        let text = String::from_utf8(wire).unwrap();
        let row = text
            .lines()
            .find_map(|line| match flea::ui_logic_op1::parse(line) {
                Some(flea::ui_logic_op1::Event::Job { job, .. }) => Some(job),
                _ => None,
            })
            .expect("the production job reporter publishes the sample");
        let card = flea::ui_logic_op1::jobs::block(&crate::i18n::test_catalogs::library_english(), &row);
        assert!((card.bar.unwrap() - expected as f64 / 15.).abs() < 1e-9);
        assert_eq!(card.head, "Extracting 2 of 2");
        assert!(card.file.starts_with("large"));
        assert!(!card.line.is_empty());
    }
}

#[test]
fn bfx264_samples_rotate_past_the_first_probe_batch() {
    let root = TestDir::new("extract-live-probes");
    let index = Index {
        known: true,
        total: SAMPLE_PROBES + 1,
        bytes: (SAMPLE_PROBES as u64 + 1) * 12,
        members: (0..=SAMPLE_PROBES)
            .map(|i| (format!("member{i}"), 12))
            .collect(),
        ..Index::default()
    };
    let name = index.members.keys().nth(SAMPLE_PROBES).unwrap().clone();
    let growing = root.file(&name, "1234");
    let mut progress = Progress::new(index, false);
    let (tx, rx) = std::sync::mpsc::channel();
    progress.sample(root.path(), 264, &tx, false);
    assert!(
        rx.try_recv().is_err(),
        "the first bounded probe batch has no file"
    );
    progress.reported = Instant::now() - PROGRESS_EVERY;
    progress.sample(root.path(), 264, &tx, false);
    match rx
        .try_recv()
        .expect("the next batch reaches the growing member")
    {
        OpMsg::JobEntries { bytes, file, .. } => {
            assert_eq!(bytes, 4);
            assert_eq!(file, name);
        }
        _ => panic!("expected extraction sample"),
    }
    std::fs::write(growing, "12345678").unwrap();
    progress.reported = Instant::now() - PROGRESS_EVERY;
    progress.sample(root.path(), 264, &tx, false);
    match rx.try_recv().expect("a found growing member stays sampled") {
        OpMsg::JobEntries { bytes, .. } => assert_eq!(bytes, 8),
        _ => panic!("expected extraction sample"),
    }
}

#[test]
fn bfx264_unknown_totals_and_password_failures_stay_honest() {
    let root = TestDir::new("extract-live-unknown");
    root.file("done", "abc");
    let growing = root.file("large", "1234");
    let mut progress = Progress::new(Index::default(), false);
    let (tx, rx) = std::sync::mpsc::channel();
    progress.line("x done", false, root.path());
    progress.preview("x large");
    progress.sample(root.path(), 264, &tx, false);
    let _ = rx.try_recv();
    std::fs::write(growing, "12345678").unwrap();
    progress.reported = Instant::now() - PROGRESS_EVERY;
    progress.sample(root.path(), 264, &tx, false);
    match rx
        .try_recv()
        .expect("unknown total still reports changing bytes")
    {
        OpMsg::JobEntries {
            done,
            total,
            bytes,
            total_bytes,
            ..
        } => assert_eq!((done, total, bytes, total_bytes), (1, 0, 11, 0)),
        _ => panic!("expected extraction sample"),
    }
    root.file("failed", "");
    let mut progress = Progress::new(Index::default(), true);
    progress.line("- failed", true, root.path());
    progress.sample(root.path(), 264, &tx, false);
    assert!(
        rx.try_recv().is_err(),
        "a wrong password's empty output claims no progress"
    );
}

// Under -bso2 7z names a member before writing it, so the named member stays current until its bytes all arrive.
#[test]
fn bfx264_a_named_member_completes_only_when_its_bytes_arrive() {
    let root = TestDir::new("extract-live-current");
    let growing = root.file("large", "1234");
    let index = Index {
        known: true,
        total: 2,
        bytes: 15,
        members: HashMap::from([("first".into(), 3), ("large".into(), 12)]),
        ..Index::default()
    };
    let mut progress = Progress::new(index, false);
    let (tx, rx) = std::sync::mpsc::channel();
    progress.line("- large", true, root.path());
    for (size, expected) in [(4, (0, 4)), (12, (1, 12))] {
        std::fs::write(&growing, vec![b'x'; size]).unwrap();
        progress.reported = Instant::now() - PROGRESS_EVERY;
        progress.sample(root.path(), 264, &tx, false);
        match rx.try_recv().expect("a sample for the named member") {
            OpMsg::JobEntries {
                done, bytes, file, ..
            } => {
                assert_eq!((done, bytes), expected, "{size} bytes staged");
                assert_eq!(file, "large");
            }
            _ => panic!("expected extraction sample"),
        }
    }
}
