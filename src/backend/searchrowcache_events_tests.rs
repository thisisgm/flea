use super::*;
use super::test_support::{assert_cleared, assert_unwatched, listing, Fixture, FOREIGN_WD};
use crate::backend::inotifyburst::EVENT_HEADER;
use crate::backend::inotifyburst::event;
use std::os::unix::fs::symlink;

#[test]
fn search_fs3ra_directory_alias_handoff_keeps_the_shared_watch_armed() {
    let root = Fixture::new();
    root.dir("real");
    root.file("real/a", "a");
    root.file("real/b", "b");
    symlink("real", root.join("alias")).unwrap();
    let mut cache = Cache::default();
    cache.window(root.path(), &listing(&["real/a"]), 0, 1, "");
    let wd = cache.watches[&root.join("real")];
    cache.window(root.path(), &listing(&["real/a", "alias/b"]), 0, 2, "");
    assert_eq!(cache.watches[&root.join("alias")], wd);
    assert_eq!(cache.refs[&wd], 2);
    cache.window(root.path(), &listing(&["alias/b"]), 0, 1, "");
    assert_eq!(cache.refs[&wd], 1);
    let fd = cache.fd.as_ref().unwrap();
    let info = std::fs::read_to_string(format!("/proc/self/fdinfo/{}", fd.as_raw_fd())).unwrap();
    // Sample input: inotify wd:1 ino:a001 sdev:800013 mask:fce ignored_mask:0 fhandle-bytes:8 fhandle-type:1 f_handle:01a0
    assert!(info.lines().any(|line| line.split_whitespace().any(|field| field == format!("wd:{wd:x}"))));
    let before = cache.misses;
    cache.window(root.path(), &listing(&["alias/b"]), 0, 1, "");
    assert_eq!(cache.misses, before);
    cache.clear();
    assert_unwatched(cache.fd.as_ref().unwrap());
}

#[test]
fn search_fs3ra_directory_self_and_malformed_events_remove_all_coverage() {
    let root = Fixture::new();
    root.file("row", "row");
    let mut events: Vec<_> = [IN_MOVE_SELF, IN_DELETE_SELF, IN_UNMOUNT]
        .into_iter().map(|mask| event(FOREIGN_WD, mask, b"")).collect();
    events.push(vec![0u8; EVENT_HEADER - 1]);
    events.push(event(FOREIGN_WD, IN_MODIFY, b"name")[..EVENT_HEADER + 1].to_vec());
    for bytes in events {
        let mut cache = Cache::default();
        cache.window(root.path(), &listing(&["row"]), 0, 1, "");
        assert!(!cache.burst(&bytes));
        assert_cleared(&cache);
    }
}
