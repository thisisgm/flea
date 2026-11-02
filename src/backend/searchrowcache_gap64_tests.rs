//! A memory-mapped write changes the same metadata the uncached viewport reports.
use super::{
    test_support::{listing, Fixture},
    Cache,
};
use std::{
    ffi::c_void,
    os::fd::AsRawFd,
    time::{Duration, UNIX_EPOCH},
};

extern "C" {
    fn mmap(
        address: *mut c_void,
        length: usize,
        protection: i32,
        flags: i32,
        fd: i32,
        offset: i64,
    ) -> *mut c_void;
    fn msync(address: *mut c_void, length: usize, flags: i32) -> i32;
    fn munmap(address: *mut c_void, length: usize) -> i32;
}

const PROT_READ: i32 = 1;
const PROT_WRITE: i32 = 2;
const MAP_SHARED: i32 = 1;
const MS_SYNC: i32 = 4;

#[test]
fn sol_gap64_mmap_write_keeps_search_and_fresh_window_mtime_equal() {
    let root = Fixture::new();
    root.file("mapped", "original contents");
    let path = root.join("mapped");
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    file.set_modified(UNIX_EPOCH + Duration::from_secs(946684800))
        .unwrap();
    let rows = listing(&["mapped"]);
    let mut cache = Cache::default();
    let before = cache.window(root.path(), &rows, 0, 1, "").0[0].mtime;
    assert!(
        !cache.entries.is_empty(),
        "the test must prime an actual cached hit"
    );
    assert_eq!(
        cache.window(root.path(), &rows, 0, 1, "").0[0].mtime,
        before
    );
    cache.remember(&path, &"r".repeat(super::BYTE_CAP - cache.bytes), "");
    assert_eq!(cache.bytes, super::BYTE_CAP, "prime a full cache");
    unsafe {
        let length = "original contents".len();
        let mapped = mmap(
            std::ptr::null_mut(),
            length,
            PROT_READ | PROT_WRITE,
            MAP_SHARED,
            file.as_raw_fd(),
            0,
        );
        assert_ne!(mapped as isize, -1, "mmap failed");
        mapped.cast::<u8>().write(b'X');
        let synced = msync(mapped, length, MS_SYNC);
        let unmapped = munmap(mapped, length);
        assert_eq!((synced, unmapped), (0, 0));
    }
    let fresh = super::super::meta::stat_range(root.path(), &rows, 0, 1).0[0].mtime;
    assert_ne!(
        fresh, before,
        "the mapped write must change filesystem mtime"
    );
    let cached = cache.window(root.path(), &rows, 0, 1, "").0[0].mtime;
    assert_eq!(
        cached, fresh,
        "search rows must agree with a fresh viewport after mmap.flush"
    );
    assert!(
        cache.entries.contains_key(&path),
        "a replacement may reuse the space freed by its invalid entry"
    );
}

#[test]
fn sol_gap64_cold_window_never_serves_or_replaces_a_prepared_row() {
    let root = Fixture::new();
    let names: Vec<_> = (0..80).map(|row| format!("row{row:03}")).collect();
    for name in &names {
        root.file(name, "row");
    }
    let rows = listing(&names.iter().map(String::as_str).collect::<Vec<_>>());
    let mut cache = Cache::default();
    cache.window(root.path(), &rows, 0, 40, "");
    cache.window(root.path(), &rows, 0, 40, "");
    let path = root.join("row035");
    cache.remember(&path, "old prepared row", "old kind");
    assert!(cache.rendered(&path).is_some());
    root.file("row035", "changed before the cold pass");
    cache.window(root.path(), &rows, 20, 40, "");
    assert!(
        cache.rendered(&path).is_none(),
        "cold metadata must not reuse a stale serialized row"
    );
    cache.remember(&path, "cold response", "new kind");
    assert_ne!(
        cache.entries[&path].rendered.as_ref().unwrap().0,
        "cold response"
    );
    cache.window(root.path(), &rows, 20, 40, "");
    assert!(
        cache.rendered(&path).is_none(),
        "the next warm pass must rebuild the changed row"
    );
}
