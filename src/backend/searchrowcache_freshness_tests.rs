use super::*;
use super::test_support::{counts, listing, snapshot, Fixture, FOREIGN_WD, MISSES, OPENS};
use std::os::unix::fs::MetadataExt;

fn current(cache: &mut Cache, root: &Fixture, rows: &Listing) -> Vec<Meta> {
    let metas = cache.window(root.path(), rows, 0, rows.len(), "").0;
    let fresh = super::super::meta::stat_range(root.path(), rows, 0, rows.len()).0;
    for (meta, expected) in metas.iter().zip(fresh) {
        assert_eq!((meta.dev, meta.size, meta.mode, meta.mtime), (expected.dev, expected.size, expected.mode, expected.mtime));
    }
    metas
}

#[test]
fn search_fs3ra_foreign_ignored_and_overflow_fail_closed_and_force_current_misses() {
    let root = Fixture::new();
    root.file("row", "old");
    for mask in [IN_IGNORED, IN_Q_OVERFLOW] {
        let mut cache = Cache::default();
        let counters = counts(&cache);
        let rows = listing(&["row"]);
        current(&mut cache, &root, &rows);
        let before = snapshot(&counters);
        assert!(!cache.burst(&super::super::inotifyburst::event(FOREIGN_WD, mask, b"")));
        assert!(cache.entries.is_empty() && cache.requested.is_empty() && cache.watches.is_empty());
        assert_eq!(cache.inodes.len(), 0);
        assert_eq!(cache.bytes, 0);
        let fresh = "current metadata";
        root.file("row", fresh);
        assert_eq!(current(&mut cache, &root, &rows)[0].size, fresh.len() as u64);
        assert_eq!(snapshot(&counters)[MISSES], before[MISSES] + 1);
        assert_eq!(snapshot(&counters)[OPENS], before[OPENS], "both descriptors stay open");
    }
}

#[test]
fn search_fs3ra_deleted_watched_directory_invalidates_without_inline_close() {
    let root = Fixture::new();
    let victim = root.dir("victim");
    let mut rows = Listing::new();
    rows.push("victim", true);
    let mut cache = Cache::default();
    let counters = counts(&cache);
    current(&mut cache, &root, &rows);
    let before = snapshot(&counters);
    std::fs::remove_dir(victim).unwrap();
    assert_eq!(current(&mut cache, &root, &rows)[0].mode, 0);
    assert_eq!(snapshot(&counters)[MISSES], before[MISSES] + 1);
    // The failed directory watch ends that window early, so only a recreated directory drains the inode queue.
    root.dir("victim");
    current(&mut cache, &root, &rows);
    assert_eq!(snapshot(&counters)[OPENS], before[OPENS], "both descriptors stay open");
}

#[test]
fn search_fs3ra_ancestor_renamed_away_and_back_never_keeps_old_facts() {
    let root = Fixture::new();
    root.dir("parent/child");
    root.file("parent/child/row", "old");
    let rows = listing(&["parent/child/row"]);
    let mut cache = Cache::default();
    let counters = counts(&cache);
    current(&mut cache, &root, &rows);
    let before = snapshot(&counters);
    std::fs::rename(root.join("parent"), root.join("away")).unwrap();
    let changed = "changed while away";
    root.file("away/child/row", changed);
    std::fs::rename(root.join("away"), root.join("parent")).unwrap();
    assert_eq!(current(&mut cache, &root, &rows)[0].size, changed.len() as u64);
    assert_eq!(snapshot(&counters)[MISSES], before[MISSES] + 1);
    assert_eq!(snapshot(&counters)[OPENS], before[OPENS], "both descriptors stay open");
}

#[test]
fn search_fs3ra_replaced_inode_is_rearmed_and_write_through_its_new_alias_invalidates() {
    let root = Fixture::new();
    root.dir("visible");
    root.dir("other");
    root.file("visible/row", "old");
    let rows = listing(&["visible/row"]);
    let mut cache = Cache::default();
    let counters = counts(&cache);
    current(&mut cache, &root, &rows);
    let old = root.join("visible/row").metadata().unwrap().ino();
    let replacement = "new inode";
    root.file("other/new", replacement);
    std::fs::hard_link(root.join("other/new"), root.join("other/alias")).unwrap();
    let new = root.join("other/new").metadata().unwrap().ino();
    assert_ne!(new, old);
    std::fs::rename(root.join("other/new"), root.join("visible/row")).unwrap();
    assert_eq!(current(&mut cache, &root, &rows)[0].size, replacement.len() as u64);
    assert!(cache.inodes.watches_inode(&root.join("visible/row")));
    let before = snapshot(&counters);
    let through_alias = "write through the new hard link";
    root.file("other/alias", through_alias);
    assert_eq!(current(&mut cache, &root, &rows)[0].size, through_alias.len() as u64);
    assert_eq!(snapshot(&counters)[MISSES], before[MISSES] + 1);
    assert_eq!(snapshot(&counters)[OPENS], before[OPENS], "both descriptors stay open");
}

#[test]
fn search_fs3ra_hard_link_handoff_does_not_remove_the_shared_watch_descriptor() {
    let root = Fixture::new();
    root.dir("a");
    root.dir("b");
    root.file("a/row", "old");
    std::fs::hard_link(root.join("a/row"), root.join("b/row")).unwrap();
    let mut cache = Cache::default();
    let counters = counts(&cache);
    current(&mut cache, &root, &listing(&["a/row"]));
    current(&mut cache, &root, &listing(&["a/row", "b/row"]));
    let before = snapshot(&counters);
    current(&mut cache, &root, &listing(&["b/row"]));
    assert_eq!(snapshot(&counters)[MISSES], before[MISSES], "B stays armed when A's path leaves");
    let before = snapshot(&counters);
    let through_a = "write through A after handoff";
    root.file("a/row", through_a);
    assert_eq!(current(&mut cache, &root, &listing(&["b/row"]))[0].size, through_a.len() as u64);
    assert_eq!(snapshot(&counters)[MISSES], before[MISSES] + 1);
    assert_eq!(snapshot(&counters)[OPENS], before[OPENS], "both descriptors stay open");
}

#[test]
fn search_fs3ra_retargeted_directory_symlink_rebinds_every_watch() {
    let root = Fixture::new();
    root.dir("real");
    root.dir("other");
    root.file("real/row", "old");
    root.file("other/row", "other target");
    std::os::unix::fs::symlink("real", root.join("alias")).unwrap();
    let rows = listing(&["alias/row"]);
    let mut cache = Cache::default();
    current(&mut cache, &root, &rows);
    std::os::unix::fs::symlink("other", root.join("alias.next")).unwrap();
    std::fs::rename(root.join("alias.next"), root.join("alias")).unwrap();
    current(&mut cache, &root, &rows);
    assert!(cache.inodes.watches_inode(&root.join("other/row")), "the row watch follows the new target");
    let written = "written through the new target";
    root.file("other/row", written);
    assert_eq!(current(&mut cache, &root, &rows)[0].size, written.len() as u64);
}

#[test]
fn search_fs3ra_failed_inode_watch_caches_nothing_until_coverage_is_whole() {
    let root = Fixture::new();
    root.dir("a");
    root.dir("other");
    root.file("a/row", "old");
    std::fs::hard_link(root.join("a/row"), root.join("other/alias")).unwrap();
    let rows = listing(&["a/row", "a/late"]);
    let mut cache = Cache::default();
    let counters = counts(&cache);
    cache.window(root.path(), &rows, 0, rows.len(), "");
    assert!(cache.entries.is_empty(), "a window whose inode coverage failed caches no row");
    let written = "written through the hard link";
    root.file("other/alias", written);
    root.file("a/late", "late");
    let before = snapshot(&counters);
    assert_eq!(current(&mut cache, &root, &rows)[0].size, written.len() as u64);
    assert_eq!(snapshot(&counters)[MISSES], before[MISSES] + rows.len());
}

#[test]
fn search_fs3ra_symlink_chain_retargeted_outside_every_watch_serves_the_new_target() {
    let root = Fixture::new();
    root.dir("real");
    root.dir("other");
    root.dir("hop");
    root.file("real/row", "old");
    root.file("other/row", "other target");
    std::os::unix::fs::symlink("../real", root.join("hop/current")).unwrap();
    std::os::unix::fs::symlink("hop/current", root.join("alias")).unwrap();
    let rows = listing(&["alias/row"]);
    let mut cache = Cache::default();
    let counters = counts(&cache);
    current(&mut cache, &root, &rows);
    let before = snapshot(&counters);
    current(&mut cache, &root, &rows);
    assert_eq!(snapshot(&counters)[MISSES], before[MISSES], "an unchanged chain serves the row from the cache");
    std::os::unix::fs::symlink("../other", root.join("hop/current.next")).unwrap();
    std::fs::rename(root.join("hop/current.next"), root.join("hop/current")).unwrap();
    assert_eq!(current(&mut cache, &root, &rows)[0].size, "other target".len() as u64);
    assert_eq!(snapshot(&counters)[MISSES], before[MISSES] + rows.len(), "the retarget re-reads the row");
}
