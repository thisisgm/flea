use super::*;
use crate::backend::searchrowcache::test_support::{Fixture, FOREIGN_WD};
use crate::backend::inotifyburst::EVENT_HEADER;
use crate::backend::watch::{IN_DELETE_SELF, IN_MODIFY, IN_MOVE_SELF, IN_Q_OVERFLOW, IN_UNMOUNT};
use crate::backend::inotifyburst::event;
use std::sync::atomic::Ordering;

#[test]
fn search_fs3ra_inode_own_ignored_is_consumed_without_invalidation() {
    let root = Fixture::new();
    root.file("a", "a");
    root.file("b", "b");
    let mut inodes = Inodes::default();
    let all = HashSet::from([root.join("a"), root.join("b")]);
    inodes.cover(&all);
    assert!(inodes.cover(&all));
    assert!(inodes.cover(&HashSet::from([root.join("b")])));
    assert!(inodes.retired.is_empty(), "the handoff drain consumes our IN_IGNORED");
    assert_eq!(inodes.len(), 1);
    assert_eq!(inodes.counts.opens.load(Ordering::Relaxed), 1);
    assert_eq!(inodes.counts.removes.load(Ordering::Relaxed), 1);
    assert_eq!(inodes.counts.closes_inline.load(Ordering::Relaxed), 0);
}

#[test]
fn search_fs3ra_inode_foreign_lost_self_and_write_events_remove_all_coverage() {
    let root = Fixture::new();
    root.file("a", "a");
    root.file("b", "b");
    for mask in [IN_IGNORED, IN_Q_OVERFLOW, IN_MOVE_SELF, IN_DELETE_SELF, IN_UNMOUNT, IN_MODIFY] {
        let mut inodes = Inodes::default();
        let all = HashSet::from([root.join("a"), root.join("b")]);
        inodes.cover(&all);
        assert!(!inodes.burst(&event(FOREIGN_WD, mask, b"")), "mask {mask:x}");
        inodes.assert_cleared();
        assert!(inodes.fd.is_some());
        assert_eq!(inodes.counts.closes_inline.load(Ordering::Relaxed), 0);
        inodes.cover(&all);
        assert!(inodes.covered(&root.join("a")) && inodes.covered(&root.join("b")));
        assert_eq!(inodes.counts.opens.load(Ordering::Relaxed), 1);
    }
}

#[test]
fn search_fs3ra_inode_truncated_events_fail_closed_on_the_same_descriptor() {
    let root = Fixture::new();
    root.file("row", "row");
    for bytes in [vec![0u8; EVENT_HEADER - 1], event(FOREIGN_WD, IN_MODIFY, b"name")[..EVENT_HEADER + 1].to_vec()] {
        let mut inodes = Inodes::default();
        inodes.cover(&HashSet::from([root.join("row")]));
        assert!(!inodes.burst(&bytes));
        inodes.assert_cleared();
        assert!(inodes.fd.is_some());
    }
}
