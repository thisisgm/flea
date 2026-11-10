use super::*;
use crate::backend::testdir::TestDir;
use std::os::unix::fs::MetadataExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

fn two_items(d: &TestDir) -> Confirmation {
    d.dir("files");
    d.dir("info");
    let mut trees = Manifest::new(d.path()).unwrap();
    let mut items = Manifest::new(d.path()).unwrap();
    for name in ["a", "b"] {
        let path = d.file(&format!("files/{name}"), "payload");
        d.file(
            &format!("info/{name}.trashinfo"),
            "[Trash Info]\nPath=/original\n",
        );
        let meta = path.symlink_metadata().unwrap();
        let mut backing =
            Reviewed::inspect(path, &format!("l{}:{}", meta.dev(), meta.ino())).unwrap();
        backing
            .snapshot(&mut trees, d.path(), &Cancellation::default())
            .unwrap();
        let item = Detail {
            item: Item {
                uri: format!("trash:///{name}"),
                original: format!("/original/{name}"),
            },
            size: 7,
            deleted: String::new(),
            identity: "id".into(),
            directory: false,
            icon: String::new(),
            backing: Some(backing),
        };
        items
            .append(save_detail(&item).unwrap().as_bytes())
            .unwrap();
    }
    Confirmation {
        token: 1,
        items,
        trees,
        listing: Vec::new(),
        targets: Manifest::new(d.path()).unwrap(),
        count: 2,
        nodes: 2,
        all: true,
        sweep: Default::default(),
    }
}

#[test]
fn cancelling_between_items_keeps_every_unstarted_payload_paired() {
    let d = TestDir::new("trash-batch-cancel");
    let confirmation = two_items(&d);
    let flag = Arc::new(AtomicBool::new(false));
    let cancellation = Cancellation::operation(flag.clone());
    d.assert_contains(&d.join("files/a"));
    d.assert_contains(&d.join("files/b"));
    let fields = delete_with(
        &confirmation,
        &Ok(d.path().to_path_buf()),
        &cancellation,
        |reached, _, _| {
            if reached == 1 {
                flag.store(true, Ordering::Relaxed);
            }
        },
    );
    let reply = format!("{{{fields}}}");
    assert_eq!(field_usize(&reply, "done"), Some(1));
    assert_eq!(field_usize(&reply, "failed"), Some(0));
    assert_eq!(field_usize(&reply, "skipped"), Some(1));
    assert!(field_bool(&reply, "cancelled"));
    assert!(!d.join("files/a").exists() && !d.join("info/a.trashinfo").exists());
    assert_eq!(
        std::fs::read_to_string(d.join("files/b")).unwrap(),
        "payload"
    );
    assert!(d.join("info/b.trashinfo").is_file());
}

#[test]
fn a_cancel_after_the_last_item_does_not_relabel_completion() {
    let d = TestDir::new("trash-late-cancel");
    let confirmation = two_items(&d);
    let flag = Arc::new(AtomicBool::new(false));
    let cancellation = Cancellation::operation(flag.clone());
    d.assert_contains(&d.join("files/a"));
    d.assert_contains(&d.join("files/b"));
    let fields = delete_with(
        &confirmation,
        &Ok(d.path().to_path_buf()),
        &cancellation,
        |reached, _, _| {
            if reached == 2 {
                flag.store(true, Ordering::Relaxed);
            }
        },
    );
    let reply = format!("{{{fields}}}");
    assert_eq!(field_usize(&reply, "done"), Some(2));
    assert!(!field_bool(&reply, "cancelled"));
    assert_eq!(field_usize(&reply, "leftover"), Some(0));
    assert_eq!(std::fs::read_dir(d.join("files")).unwrap().count(), 0);
    assert_eq!(std::fs::read_dir(d.join("info")).unwrap().count(), 0);
}

#[test]
fn late_cancel_keeps_job_completion_and_legacy_replies_consistent() {
    let (tx, _) = channel();
    let mut ops = Ops::new(tx.clone());
    let (id, cancel) = ops.claim_transfer();
    let mut out = Vec::new();
    ops.jobs.adopt(
        &mut out,
        id,
        "emptyTrash",
        "Emptying Trash",
        2,
        cancel.clone(),
    );
    cancel.store(true, Ordering::Relaxed);
    let progress = Progress {
        id,
        tx,
        count: Cell::new(2),
        nodes: Cell::new(2),
        last: Cell::new(None),
        named: Default::default(),
    };
    let reply = r#"{"t":"trashbrowse","id":9,"op":"delete","ok":true,"done":2,"failed":0,"failures":[],"skipped":0,"cancelled":false}"#;
    let message = progress.finish(reply.into(), true);
    let expected = format!(
        "{}\n{reply}",
        crate::backend::opsreq::transferdone_line(id, 2, 0, 0, false, &[], false, "", &[])
    );
    match &message {
        OpMsg::JobDone { line, error, .. } => {
            assert_eq!(line, &expected);
            assert!(error.is_empty());
        }
        _ => panic!("an adopted deletion must use the common terminal reporter"),
    }
    out.clear();
    crate::backend::opsdispatch::report_op(&mut out, &mut ops, message);
    let text = String::from_utf8(out).unwrap();
    assert!(text.ends_with(&format!("{expected}\n")), "{text}");
    assert!(
        !ops.jobs.rows[0].cancelled,
        "a late flag must not relabel completed deletion"
    );
    assert!(ops.jobs.rows[0].state == crate::backend::jobs::State::Done);
    assert_eq!(ops.jobs.rows[0].done, 2);
    assert!(ops.live.running().is_none());
}

#[test]
fn cancel_before_claim_counts_untouched_items_as_skipped() {
    let d = TestDir::new("trash-preclaim-cancel");
    let confirmation = two_items(&d);
    let flag = Arc::new(AtomicBool::new(false));
    let cancel = Cancellation::operation(flag.clone());
    for name in ["a", "b"] {
        d.assert_contains(&d.join(&format!("files/{name}")));
        d.assert_contains(&d.join(&format!("info/{name}.trashinfo")));
    }
    let fields = delete_with(
        &confirmation,
        &Ok(d.path().to_path_buf()),
        &cancel,
        |_, item, _| {
            if item.is_some() {
                flag.store(true, Ordering::Relaxed);
            }
        },
    );
    let reply = format!("{{{fields}}}");
    for name in ["a", "b"] {
        assert_eq!(std::fs::read_to_string(d.join(&format!("files/{name}"))).unwrap(), "payload");
        assert!(d.join(&format!("info/{name}.trashinfo")).is_file());
    }
    assert!(field_bool(&reply, "cancelled"));
    assert_eq!(field_usize(&reply, "done"), Some(0));
    assert_eq!(field_usize(&reply, "failed"), Some(0), "untouched files are skipped: {reply}");
    assert_eq!(field_usize(&reply, "skipped"), Some(2));
}

#[test]
fn cancelling_inside_a_tree_keeps_the_partial_item_failed() {
    let d = TestDir::new("trash-partial-batch-cancel");
    let mut confirmation = two_items(&d);
    let path = d.dir("files/tree");
    d.file("files/tree/first", "abc");
    d.file("files/tree/second", "de");
    d.file("info/tree.trashinfo", "[Trash Info]\nPath=/original/tree\n");
    let meta = path.symlink_metadata().unwrap();
    let mut backing = Reviewed::inspect(path.clone(), &format!("l{}:{}", meta.dev(), meta.ino())).unwrap();
    backing.snapshot(&mut confirmation.trees, d.path(), &Cancellation::default()).unwrap();
    let item = Detail {
        item: Item { uri: "trash:///tree".into(), original: "/original/tree".into() },
        size: 5,
        deleted: String::new(),
        identity: "id".into(),
        directory: true,
        icon: String::new(),
        backing: Some(backing),
    };
    confirmation.items.append(save_detail(&item).unwrap().as_bytes()).unwrap();
    confirmation.count += 1;
    confirmation.nodes += 3;
    let flag = Arc::new(AtomicBool::new(false));
    d.assert_contains(&path);
    for name in ["a", "b", "tree"] {
        d.assert_contains(&d.join(&format!("files/{name}")));
        d.assert_contains(&d.join(&format!("info/{name}.trashinfo")));
    }
    let fields = delete_with(
        &confirmation,
        &Ok(d.path().to_path_buf()),
        &Cancellation::operation(flag.clone()),
        |removed, _, _| {
            if removed == 3 {
                flag.store(true, Ordering::Relaxed);
            }
        },
    );
    let reply = format!("{{{fields}}}");
    assert_eq!(field_usize(&reply, "done"), Some(2), "{reply}");
    assert_eq!(field_usize(&reply, "failed"), Some(1), "{reply}");
    assert_eq!(field_usize(&reply, "skipped"), Some(0), "{reply}");
    assert!(field_bool(&reply, "cancelled"));
    assert_eq!(std::fs::read_dir(&path).unwrap().count(), 1);
    assert!(d.join("info/tree.trashinfo").is_file());
    assert_eq!(std::fs::read_dir(d.join("files")).unwrap().count(), 1);
}

#[test]
fn cancellation_does_not_hide_a_recovery_error() {
    let d = TestDir::new("trash-cancel-recovery-error");
    let confirmation = two_items(&d);
    let flag = Arc::new(AtomicBool::new(false));
    let fields = delete_with(
        &confirmation,
        &Err("Recovery directory unavailable.".into()),
        &Cancellation::operation(flag.clone()),
        |_, item, _| {
            if item.is_some() {
                flag.store(true, Ordering::Relaxed);
            }
        },
    );
    let reply = format!("{{{fields}}}");
    assert_eq!(field_usize(&reply, "done"), Some(0));
    assert_eq!(field_usize(&reply, "failed"), Some(1));
    assert_eq!(field_usize(&reply, "skipped"), Some(1));
    assert!(field_bool(&reply, "cancelled"));
    assert!(reply.contains("Recovery directory unavailable."));
    for name in ["a", "b"] {
        assert_eq!(std::fs::read_to_string(d.join(&format!("files/{name}"))).unwrap(), "payload");
        assert!(d.join(&format!("info/{name}.trashinfo")).is_file());
    }
}

// TR2-c: the delete reply counts what Empty Trash's sweep of unlisted entries left on disk, so the footer can say it.
#[test]
fn the_reply_counts_left_out_entries_the_sweep_could_not_remove() {
    let d = TestDir::new("trash-leftover");
    let mut confirmation = two_items(&d);
    d.file("info/c.trashinfo", "[Trash Info]\nPath=/original/c\n");
    let listed = ["a", "b"].map(|name| Item {
        uri: format!("trash:///{name}"),
        original: format!("/original/{name}"),
    });
    let files = d.join("files");
    let read = |_: &[&str]| -> Result<String, Refusal> {
        Ok(["a", "b"]
            .map(|name| {
                let uri = crate::backend::thumbcache::uri_for(&files.join(name));
                format!("trash:///{name}\t1\t(regular)\tstandard::target-uri={uri}\n")
            })
            .concat())
    };
    let scratch = d.dir("scratch");
    confirmation.sweep = super::super::sweep::Sweep::review(
        &listed,
        &mut confirmation.trees,
        &scratch,
        &Cancellation::default(),
        read,
    )
    .unwrap();
    // A claim's quarantine that appears after the review keeps its Trash's records.
    d.dir("files/.flea-delete-9-9");
    let fields = delete_with(
        &confirmation,
        &Ok(d.path().to_path_buf()),
        &Cancellation::default(),
        |_, _, _| {},
    );
    let reply = format!("{{{fields}}}");
    assert_eq!(field_usize(&reply, "done"), Some(2));
    assert_eq!(field_usize(&reply, "failed"), Some(0));
    assert_eq!(field_usize(&reply, "leftover"), Some(1));
    assert!(d.join("info/c.trashinfo").is_file());
}

// EY-501: the Emptying Trash card's item line keeps the item in flight; a sample between two items never blanks it.
#[test]
fn fix_fileops_2_emptying_samples_keep_the_item_in_flight() {
    let d = TestDir::new("trash-progress-names");
    let confirmation = two_items(&d);
    let (tx, rx) = channel();
    let progress = Progress {
        id: 1,
        tx,
        count: Cell::new(2),
        nodes: Cell::new(2),
        last: Cell::new(None),
        named: Default::default(),
    };
    progress.sample("deleting", 0, 2, None, true);
    delete_with(
        &confirmation,
        &Ok(d.path().to_path_buf()),
        &Cancellation::default(),
        |done, item, _| progress.sample("deleting", done, 2, item, true),
    );
    let files: Vec<_> = rx
        .try_iter()
        .filter_map(|message| match message {
            OpMsg::JobEntries {
                file, item_bytes, ..
            } => Some((file, item_bytes)),
            _ => None,
        })
        .collect();
    let (first, named) = files.split_first().unwrap();
    assert_eq!(first, &(String::new(), 0), "nothing is in flight yet");
    assert!(
        named
            .iter()
            .all(|(file, size)| !file.is_empty() && *size == 7),
        "a sample between items blanks the card's item line: {files:?}"
    );
    assert_eq!(named.last().unwrap().0, "b");
}
