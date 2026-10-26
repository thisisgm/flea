use super::*;
use crate::backend::searchwatch::Watch;
use crate::backend::testdir::TestDir;
use std::os::unix::fs::PermissionsExt;

#[test]
fn search_a_subtree_gone_before_its_read_is_not_unreadable() {
    for watched in [false, true] {
        for replacement in ["absent", "file", "symlink"] {
            let d = TestDir::new("subtreegone");
            let target = TestDir::new("subtreegone-target");
            if replacement == "file" { d.file("gone", ""); }
            if replacement == "symlink" { std::os::unix::fs::symlink(target.path(), d.join("gone")).unwrap(); }
            let (events, _receiver) = crate::backend::events::channel();
            let watch = watched.then(|| Watch::new(d.path(), false, events));
            let core = watch.as_ref().map(|watch| watch.core.as_ref());
            if let Some(core) = core {
                assert!(core.arm_local(Path::new("gone"), ""), "a vanished descendant is not a registration failure");
                assert!(!core.drain().failed, "the real watch has not reported a failure");
            }
            let mut index = Index::new();
            assert!(index.push("gone", true));
            index.fill_subtrees(d.path(), &[b"gone".to_vec()], false, "", core);
            assert_eq!(index.partial, None, "a vanished or replaced local directory is not unreadable");
            assert_eq!(index.len(), 1, "the parent patch owns the discovered row");
            assert!(index.patch_watched(d.path(), Path::new(""), false, "", core));
            assert_eq!(index.len(), usize::from(replacement != "absent"));
            if index.len() != 0 { assert!(!index.is_dir(0), "the parent reconciles the new kind"); }
            assert_eq!(index.partial, None);
            if let Some(core) = core {
                core.drain();
                if replacement == "file" { std::fs::write(d.join("gone"), "x").unwrap(); }
                if replacement == "symlink" { target.file("probe", ""); }
                d.file("probe", "");
                core.flush();
                let dirs = core.drain().dirs;
                assert_eq!(dirs, [std::path::PathBuf::new()].into(), "{replacement}: no watch was registered on gone, so events stay the root's");
            }
        }
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

#[test]
fn search_a_permission_denied_local_subtree_stays_partial() {
    for watched in [false, true] {
        let d = TestDir::new("subtree-permission-denied");
        let denied = d.dir("denied");
        d.file("denied/needle", "");
        assert!(d.join(".flea-test-sandbox").is_file());
        d.assert_contains(&denied);
        std::fs::set_permissions(&denied, std::fs::Permissions::from_mode(0o000)).unwrap();
        if std::fs::read_dir(&denied).is_ok() {
            eprintln!("skipped: this process overrides directory permissions, so mode 000 cannot produce EACCES");
            return;
        }
        let error = crate::backend::searchread::read(&denied).err().expect("chmod 000 must refuse the real directory read");
        assert_eq!(error.raw_os_error(), Some(13), "this fixture must exercise EACCES");
        assert!(read_bounded(&denied, "", false).is_err(), "EACCES must not become a vanished directory");
        let (events, _receiver) = crate::backend::events::channel();
        let watch = watched.then(|| Watch::new(d.path(), false, events));
        let core = watch.as_ref().map(|watch| watch.core.as_ref());
        let mut index = Index::new();
        assert!(index.push("denied", true));
        index.fill_subtrees(d.path(), &[b"denied".to_vec()], false, "", core);
        let reason = if watched { "watch_failed" } else { "unreadable" };
        assert_eq!(index.partial, Some(reason), "the first real failure stays reported");
        assert_eq!(index.len(), 1, "the unreadable subtree's descendants were not invented");
        d.assert_contains(&denied);
    }
}
