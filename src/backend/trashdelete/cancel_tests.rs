use super::*;
use crate::backend::testdir::TestDir;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

#[test]
fn cancelling_inside_a_tree_puts_its_surviving_payload_and_metadata_back_together() {
    let d = TestDir::new("trash-tree-cancel");
    let path = d.dir("files/tree");
    d.dir("info");
    d.file("files/tree/a", "first");
    d.file("files/tree/b", "second");
    let info = d.file("info/tree.trashinfo", "[Trash Info]\nPath=/original/tree\n");
    let meta = path.symlink_metadata().unwrap();
    let mut reviewed =
        Reviewed::inspect(path.clone(), &format!("l{}:{}", meta.dev(), meta.ino())).unwrap();
    let mut manifest = Manifest::new(d.path()).unwrap();
    reviewed
        .snapshot(&mut manifest, d.path(), &Cancellation::default())
        .unwrap();
    let restored = Reviewed::from_saved(&reviewed.saved().unwrap(), &manifest).unwrap();
    assert_eq!(restored.reviewed_nodes(), 3);
    assert_eq!(restored.reviewed_bytes(), 11);
    let flag = Arc::new(AtomicBool::new(false));
    let cancel = Cancellation::operation(flag.clone());
    let mut nodes = 0;
    d.assert_contains(&path);
    d.assert_contains(&info);
    let result = reviewed.delete_cancellable(d.path(), &cancel, || {
        nodes += 1;
        flag.store(true, Ordering::Relaxed);
    });
    assert!(result.is_err());
    assert_eq!(nodes, 1, "the next tree node polls cancellation");
    assert!(
        path.is_dir() && info.is_file(),
        "survivors and their metadata stay paired"
    );
    assert_eq!(std::fs::read_dir(&path).unwrap().count(), 1);
    assert_eq!(
        std::fs::read_dir(d.join("files")).unwrap().count(),
        1,
        "no quarantine is stranded"
    );
    assert_eq!(
        std::fs::read_to_string(info).unwrap(),
        "[Trash Info]\nPath=/original/tree\n"
    );
}

#[test]
fn cancellation_before_claim_preserves_everything() {
    let d = TestDir::new("trash-early-cancel");
    d.dir("files");
    d.dir("info");
    let path = d.file("files/item", "keep");
    let info = d.file("info/item.trashinfo", "[Trash Info]\nPath=/original/item\n");
    let meta = path.symlink_metadata().unwrap();
    let mut reviewed =
        Reviewed::inspect(path.clone(), &format!("l{}:{}", meta.dev(), meta.ino())).unwrap();
    reviewed
        .snapshot(
            &mut Manifest::new(d.path()).unwrap(),
            d.path(),
            &Cancellation::default(),
        )
        .unwrap();
    let flag = Arc::new(AtomicBool::new(true));
    d.assert_contains(&path);
    assert!(reviewed
        .delete_cancellable(d.path(), &Cancellation::operation(flag), || panic!(
            "no node removed"
        ))
        .is_err());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "keep");
    assert!(info.is_file());
}

#[test]
fn cancelling_after_claim_is_skipped_only_when_rollback_succeeds() {
    for (blocked, record_left) in [(false, false), (true, false), (false, true)] {
        let d = TestDir::new("trash-claimed-cancel");
        d.dir("files");
        d.dir("info");
        let path = d.file("files/item", "keep");
        let info = d.file("info/item.trashinfo", "[Trash Info]\nPath=/original/item\n");
        let meta = path.symlink_metadata().unwrap();
        let mut reviewed = Reviewed::inspect(path.clone(), &format!("l{}:{}", meta.dev(), meta.ino())).unwrap();
        reviewed.snapshot(&mut Manifest::new(d.path()).unwrap(), d.path(), &Cancellation::default()).unwrap();
        let flag = Arc::new(AtomicBool::new(false));
        let cancel = Cancellation::operation(flag.clone());
        let mut replaced = None;
        d.assert_contains(&path);
        d.assert_contains(&info);
        let refusal = reviewed.with_claimed(
            Op::Delete,
            d.path(),
            None,
            || flag.store(true, Ordering::Relaxed),
            |quarantine| {
                if blocked {
                    d.file("files/item", "replacement");
                }
                // A swapped record is a failure root cannot bypass, unlike a read-only recovery folder.
                if record_left {
                    let records: Vec<_> = std::fs::read_dir(d.path()).unwrap().map(|entry| entry.unwrap().path())
                        .filter(|path| path.extension().is_some_and(|extension| extension == "review")).collect();
                    assert_eq!(records.len(), 1, "one recovery record while claimed: {records:?}");
                    d.replace_file(&records[0], "replacement record");
                    replaced = Some(records[0].clone());
                }
                remove_tree_cancellable(
                    reviewed.tree.as_ref().unwrap(),
                    quarantine,
                    Refusal::new(Outcome::NothingDeleted, "Trash changed."),
                    &cancel,
                    &mut || panic!("no node may be deleted"),
                )
            },
        ).unwrap_err();
        if blocked {
            let Outcome::Stuck(Op::Delete, kept) = refusal.outcome() else {
                panic!("a failed rollback must remain a failure: {refusal:?}");
            };
            d.assert_contains(kept);
            assert_eq!(std::fs::read_to_string(kept.join("payload")).unwrap(), "keep");
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "replacement");
        } else if record_left {
            assert_eq!(refusal.outcome(), &Outcome::RecordLeft(None));
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "keep");
            let record = replaced.as_ref().expect("the record was swapped while claimed");
            assert_eq!(std::fs::read_to_string(record).unwrap(), "replacement record", "the changed record is preserved");
        } else {
            assert_eq!(refusal.outcome(), &Outcome::Cancelled);
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "keep");
            assert_eq!(std::fs::read_dir(d.join("files")).unwrap().count(), 1);
        }
        assert!(info.is_file());
    }
}
