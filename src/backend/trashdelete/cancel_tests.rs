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
