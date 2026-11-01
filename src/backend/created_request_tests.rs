use super::*;
use crate::backend::testdir::TestDir;
use crate::json::{field_str, field_usize};
use std::sync::mpsc::channel;

fn receipt(text: &str, request: usize) -> &str {
    text.lines()
        .find(|line| {
            field_str(line, "op").as_deref() == Some("creation")
                && field_usize(line, "id") == Some(request)
        })
        .expect("correlated receipt")
}

#[test]
fn creation_receipts_keep_legacy_mkdir_payload_and_exact_failure_identity() {
    let root = TestDir::new("creation-request");
    let (tx, _) = channel();
    let mut ops = Ops::new(tx);
    let mut out = Vec::new();
    super::super::do_mkdir(
        &mut out,
        &mut ops,
        root.path().to_str().unwrap(),
        "legacy",
        None,
    );
    let legacy = String::from_utf8(out).unwrap();
    assert!(legacy.contains(r#""t":"made","ok":true,"path":"#));
    assert!(!legacy.contains(r#""request":"#) && !legacy.contains(r#""op":"creation""#));
    let mut out = Vec::new();
    super::super::do_mkdir_requested(
        &mut out,
        &mut ops,
        root.path().to_str().unwrap(),
        "new",
        None,
        47,
    );
    let text = String::from_utf8(out).unwrap();
    let made = text
        .lines()
        .find(|line| field_str(line, "t").as_deref() == Some("made"))
        .unwrap();
    assert_eq!(field_usize(made, "request"), Some(47));
    assert_eq!(
        field_str(receipt(&text, 47), "path").as_deref(),
        root.join("new").to_str()
    );
    let mut out = Vec::new();
    super::super::do_mkdir_requested(
        &mut out,
        &mut ops,
        root.path().to_str().unwrap(),
        "new",
        None,
        48,
    );
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains(r#""t":"error","where":"mkdir""#));
    assert!(receipt(&text, 48).contains(r#""ok":false"#));
    assert_eq!(ops.journal.len(), 2, "refusal must not add a journal entry");
}

#[test]
fn creation_receipt_names_the_localized_default_a_collision_chose() {
    let root = TestDir::new("creation-request-default-base");
    root.dir("新建文件夹");
    let (tx, _) = channel();
    let mut ops = Ops::new(tx);
    let mut out = Vec::new();
    super::super::do_mkdir_requested(
        &mut out,
        &mut ops,
        root.path().to_str().unwrap(),
        "",
        Some("新建文件夹"),
        49,
    );
    let text = String::from_utf8(out).unwrap();
    let chosen = root.join("新建文件夹 2");
    assert!(receipt(&text, 49).contains(r#""ok":true"#), "{text}");
    assert_eq!(
        field_str(receipt(&text, 49), "path").as_deref(),
        chosen.to_str()
    );
    assert!(chosen.is_dir() && !root.join("New Folder").exists());
}

#[test]
fn creation_duplicate_busy_reply_does_not_steal_earlier_success_token() {
    let root = TestDir::new("duplicate-request");
    let source = root.file("notes.txt", "keep");
    let (tx, rx) = channel();
    let mut ops = Ops::new(tx);
    let mut out = Vec::new();
    duplicate(&mut out, &mut ops, source.to_str().unwrap(), 0, 61);
    duplicate(&mut out, &mut ops, source.to_str().unwrap(), 0, 62);
    let refused = String::from_utf8(out).unwrap();
    assert!(receipt(&refused, 62).contains(r#""ok":false"#));
    assert!(!refused.contains(r#""id":61"#));
    let msg = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let paths = crate::backend::searchreq::op_write_paths(&msg);
    assert_eq!(
        paths.len(),
        1,
        "the result envelope must preserve search-index paths"
    );
    let mut out = Vec::new();
    report_op(&mut out, &mut ops, msg);
    let text = String::from_utf8(out).unwrap();
    assert!(receipt(&text, 61).contains(r#""ok":true"#));
    assert_eq!(std::fs::read_to_string(&paths[0]).unwrap(), "keep");
    assert!(ops.live.running().is_none());
    assert_eq!(ops.journal.len(), 1, "one original duplicate journals once");
}

#[test]
fn creation_slow_mkdir_retains_pending_claim_until_correlated_publication() {
    let root = TestDir::new("slow-create-request");
    let _body = crate::backend::iomount::test_hold_body(format!(
        "1 0 8:1 / / rw - ext4 /dev/a rw\n30 1 0:45 / {} rw - nfs n:/s rw\n",
        root.path().display()
    ));
    let path = root.path().to_owned();
    let (tx, rx) = channel();
    let mut ops = Ops::new(tx);
    let (release, wait) = channel();
    let mut out = Vec::new();
    mkdir_with(
        &mut out,
        &mut ops,
        root.path().to_str().unwrap(),
        77,
        Duration::from_millis(1),
        move || {
            wait.recv_timeout(Duration::from_secs(5)).unwrap();
            crate::backend::ops::mkdir(&path, "later")
        },
    );
    assert_eq!(ops.pending.len(), 1);
    assert!(!String::from_utf8_lossy(&out).contains(r#""op":"creation""#));
    release.send(()).unwrap();
    let msg = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(crate::backend::searchreq::op_write_paths(&msg).contains(&root.join("later")));
    report_op(&mut out, &mut ops, msg);
    let text = String::from_utf8(out).unwrap();
    assert!(receipt(&text, 77).contains(r#""ok":true"#));
    assert!(ops.pending.is_empty());
    assert_eq!(ops.journal.len(), 1);
}
