use super::*;
use crate::backend::testdir::TestDir;
use std::sync::mpsc;
use std::time::Instant;

#[path = "airdrop_identity_tests.rs"]
mod identity;

fn send(paths: Vec<String>) -> Send {
    Send {
        peer: "3a9f0c2be41d".into(),
        paths,
        request: 7,
        items: Vec::new(),
    }
}

// A stand-in owly: logs its argv, then plays one script of stderr steps, stdout verdict and exit code.
fn stand_in(root: &TestDir, body: &str) -> String {
    let log = root.join("argv.log");
    let script = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n{}\n",
        log.display(),
        body
    );
    root.script("owly", &script).to_string_lossy().into_owned()
}

fn bytes(rx: &mpsc::Receiver<OpMsg>) -> Vec<(u64, u64)> {
    rx.try_iter()
        .filter_map(|m| match m {
            OpMsg::JobEntries {
                id: 3,
                bytes,
                total_bytes,
                ..
            } => Some((bytes, total_bytes)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_delivered_send_reports_owlys_byte_progress_and_passes_files_after_the_double_dash() {
    let root = TestDir::new("airdrop-delivered");
    let owly = stand_in(
        &root,
        r#"printf 'looking for "x" for 30 s...\nsending to "x"\naccepted\n\r100 / 400 bytes\r400 / 400 bytes\n' >&2; echo delivered; exit 0"#,
    );
    let (tx, rx) = mpsc::channel();
    let file = root.file("-photo.png", "x").to_string_lossy().into_owned();
    let verdict = run(
        Some(owly),
        3,
        &send(vec![file.clone()]),
        &AtomicBool::new(false),
        &tx,
    );
    assert_eq!(verdict, ("delivered".to_string(), String::new()));
    assert_eq!(bytes(&rx), [(100, 400), (400, 400)]);
    let argv = std::fs::read_to_string(root.join("argv.log")).unwrap();
    assert_eq!(argv, format!("send\n--to=3a9f0c2be41d\n--\n{file}\n"));
}

#[test]
fn a_declined_or_failed_send_keeps_the_devices_answer_and_owlys_reason() {
    let root = TestDir::new("airdrop-refused");
    let (tx, _rx) = mpsc::channel();
    let declined = stand_in(&root, "echo declined; exit 1");
    assert_eq!(
        run(
            Some(declined),
            3,
            &send(vec!["/x".into()]),
            &AtomicBool::new(false),
            &tx
        )
        .0,
        "declined"
    );
    let gone = stand_in(&root, "echo 'owly: no such receiver found' >&2; exit 2");
    let verdict = run(
        Some(gone),
        3,
        &send(vec!["/x".into()]),
        &AtomicBool::new(false),
        &tx,
    );
    assert_eq!(
        verdict,
        ("failed".to_string(), "no such receiver found".to_string())
    );
    let radio = stand_in(
        &root,
        "echo 'owly: awdl0: ENODEV: No such device' >&2; exit 2",
    );
    assert_eq!(
        run(
            Some(radio),
            3,
            &send(vec!["/x".into()]),
            &AtomicBool::new(false),
            &tx
        )
        .1,
        "awdl0: ENODEV: No such device"
    );
}

#[test]
fn a_missing_owly_says_so_and_a_cancel_stops_a_send_in_flight() {
    let (tx, _rx) = mpsc::channel();
    assert_eq!(
        run(
            None,
            3,
            &send(vec!["/x".into()]),
            &AtomicBool::new(false),
            &tx
        )
        .0,
        "missing"
    );
    let root = TestDir::new("airdrop-cancel");
    let slow = stand_in(&root, "exec sleep 30");
    let cancel = std::sync::Arc::new(AtomicBool::new(false));
    let flag = std::sync::Arc::clone(&cancel);
    let stopper = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        flag.store(true, Ordering::Relaxed);
    });
    let started = Instant::now();
    assert_eq!(
        run(Some(slow), 3, &send(vec!["/x".into()]), &cancel, &tx).0,
        "cancelled"
    );
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the cancel waited for owly"
    );
    stopper.join().unwrap();
}

#[test]
fn the_reply_line_names_its_request_and_only_delivered_is_ok() {
    assert_eq!(
        sent_line(4, "delivered", ""),
        r#"{"t":"airdropsent","id":4,"ok":true,"code":"delivered","reason":""}"#
    );
    assert_eq!(
        sent_line(4, "failed", "a \"b\""),
        r#"{"t":"airdropsent","id":4,"ok":false,"code":"failed","reason":"a \"b\""}"#
    );
}

#[test]
fn owly_is_found_on_path_as_a_shell_finds_it_and_a_folder_named_owly_is_not() {
    let root = TestDir::new("airdrop-find");
    let found = stand_in(&root, "exit 0");
    let missing = root.join("none");
    let plain = TestDir::new("airdrop-find-plain");
    plain.file("owly", "not a program");
    let path = std::env::join_paths([
        missing.clone(),
        plain.path().to_path_buf(),
        root.path().to_path_buf(),
    ])
    .unwrap();
    assert_eq!(
        airdrop::find(&path),
        Some(found),
        "a non-executable owly is skipped"
    );
    std::fs::create_dir_all(missing.join("owly")).unwrap();
    let only_folder = std::env::join_paths([missing]).unwrap();
    assert_eq!(airdrop::find(&only_folder), None);
}

#[test]
fn the_windows_send_line_parses_as_an_airdrop_request_and_the_facts_carry_owly() {
    let line = airdrop::send_line(3, "3a9f", "Jane's \"iPhone\"", &["/a/b c.png".to_string()]);
    match crate::backend::proto::parse_request(&line) {
        crate::backend::proto::Request::AirDrop {
            peer,
            name,
            paths,
            id,
        } => {
            assert_eq!(
                (peer.as_str(), name.as_str(), paths, id),
                (
                    "3a9f",
                    "Jane's \"iPhone\"",
                    vec!["/a/b c.png".to_string()],
                    3
                )
            );
        }
        _ => panic!("{line} did not parse as an AirDrop send"),
    }
    let facts = crate::backend::providers::facts();
    let parsed = flea::ui_logic_send_share::providers::parse_facts(&facts).expect("facts parse");
    assert!(facts.contains(r#""owly":{"installed":"#), "{facts}");
    assert_eq!(
        parsed.owly_command.is_empty(),
        airdrop::find(&std::env::var_os("PATH").unwrap_or_default()).is_none()
    );
}

#[test]
fn a_failed_job_says_a_sentence_and_owlys_reason_wins() {
    assert_eq!(job_reason("declined", ""), "The device declined the files.");
    assert_eq!(job_reason("cancelled", ""), "Send cancelled.");
    assert_eq!(job_reason("missing", ""), "AirDrop needs the owly package.");
    assert_eq!(
        job_reason("failed", ""),
        "AirDrop could not finish the transfer."
    );
    assert_eq!(job_reason("failed", "timed out"), "timed out");
}

#[test]
fn the_job_card_names_the_device_the_window_chose_not_its_service_id() {
    let mut jobs = crate::backend::jobs::Jobs::default();
    let (tx, _rx) = mpsc::channel();
    let mut out = Vec::new();
    for (job, name) in [(1, "Jane's iPhone"), (2, "")] {
        let work = crate::backend::jobwork::Work::Send {
            peer: "3a9f0c2be41d".into(),
            name: name.into(),
            paths: vec!["/nonexistent/flea-airdrop-head/a.png".into()],
            request: job,
            selection: Some(Vec::new()),
            airdrop: true,
        };
        let spec = crate::backend::jobwork::Spec {
            kind: "send",
            label: "Sending",
            target: None,
        };
        jobs.submit(
            &mut out,
            job,
            spec,
            work,
            &crate::backend::opscancel::DetachedJobs::new(),
            &tx,
        );
    }
    let targets: Vec<&str> = jobs.rows.iter().map(|job| job.target.as_str()).collect();
    assert_eq!(
        targets,
        ["Jane's iPhone", "3a9f0c2be41d"],
        "an older window's send without a name keeps the ID"
    );
}
