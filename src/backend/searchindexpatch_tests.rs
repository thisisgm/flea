use super::*;
use crate::backend::testdir::TestDir;

#[test]
fn search_a_subtree_gone_before_its_read_is_not_unreadable() {
    for replacement in ["absent", "file", "symlink"] {
        let d = TestDir::new("subtreegone");
        if replacement == "file" { d.file("gone", ""); }
        if replacement == "symlink" { std::os::unix::fs::symlink(d.path(), d.join("gone")).unwrap(); }
        let mut index = Index::new();
        assert!(index.push("gone", true));
        index.fill_subtrees(d.path(), &[b"gone".to_vec()], false, "", None);
        assert_eq!(index.partial, None, "a vanished or replaced local directory is not unreadable");
        assert_eq!(index.len(), 1, "the parent patch owns the discovered row");
        assert!(index.patch_dir(d.path(), Path::new(""), false, ""));
        assert_eq!(index.len(), usize::from(replacement != "absent"));
        if index.len() != 0 { assert!(!index.is_dir(0), "the parent reconciles the new kind"); }
        assert_eq!(index.partial, None);
    }
}

#[test]
fn search_a_missing_scope_or_remote_subtree_stays_unreadable() {
    let d = TestDir::new("subtreerefused");
    let mut index = Index::new();
    index.fill_subtrees(&d.join("absent"), &[Vec::new()], false, "", None);
    assert_eq!(index.partial, Some("unreadable"), "a vanished scope is still a failed read");
    let body = format!("1 0 0:1 / {} rw - nfs server:/ rw\n", d.path().display());
    let mut index = Index::new();
    index.fill_subtrees(d.path(), &[b"gone".to_vec()], false, &body, None);
    assert_eq!(index.partial, Some("unreadable"), "a remote snapshot has no parent watch to reconcile it");
    let mut index = Index::new();
    index.fill_subtrees(d.path(), &[b"invalid\0name".to_vec()], false, "", None);
    assert_eq!(index.partial, Some("unreadable"), "other read failures are still reported");
}
