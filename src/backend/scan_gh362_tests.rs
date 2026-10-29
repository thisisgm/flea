use super::*;
use crate::backend::{mountinfo::dev_majmin, testdir::TestDir};

#[test]
fn gh362_a_local_scan_carries_its_device_into_the_window_thread_gate() {
    let d = TestDir::new("gh362-scan-device");
    d.file("file.txt", "file contents");
    let device = d.path().metadata().unwrap().dev();
    let (listing, _) = scan(d.path().to_str().unwrap(), false).unwrap();
    assert_eq!(
        listing.base_dev, device,
        "the initial scan carries its directory device before any metadata pass"
    );
    let body = format!("1 0 {} / / rw - vfat /dev/test rw\n", dev_majmin(device));
    assert!(
        !listing.threaded_cached_with(&body),
        "a FAT window stays serial even when the first pass was fast"
    );
    assert_eq!(listing.len(), 1);
    assert_eq!(listing.name(0), "file.txt");
}
