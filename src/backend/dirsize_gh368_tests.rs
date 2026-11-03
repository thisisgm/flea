use super::*;
use crate::backend::{iomount, meta::stat_all, testdir::TestDir};
use std::os::unix::fs::MetadataExt;

fn mounts(path: &Path, kind: &str) -> String {
    let escaped = path
        .to_string_lossy()
        .replace('\\', "\\134")
        .replace(' ', "\\040");
    format!("1 0 8:1 / / rw - ext4 /dev/root rw\n2 1 0:42 / {escaped} rw - {kind} remote: rw\n")
}

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}

#[test]
fn gh368_display_sizes_never_recurse_into_a_remote_mount_root() {
    let d = TestDir::new("gh368-root");
    let remote = d.dir("One Drive");
    d.file(
        "One Drive/remote-file",
        "remote contents must never be counted",
    );
    let own = remote.symlink_metadata().unwrap().size();
    for kind in ["fuse.rclone", "nfs4", "cifs", "fuse.gvfsd-fuse"] {
        let _mounts = iomount::test_hold_body(mounts(&remote, kind));
        for size in [
            walk_until(&remote, deadline()),
            walk_cancellable(&remote, deadline(), &|| false),
        ] {
            assert!(
                size.partial,
                "{kind}: a skipped remote total must be partial"
            );
            assert_eq!(size.bytes, own, "{kind}: only the directory entry is sized");
        }
        assert!(
            walk_while(&remote, &|| false).bytes > own,
            "explicit operation counting still walks its source"
        );
    }
    for kind in ["ext4", "vfat", "fuse.gocryptfs", "fuse.mergerfs"] {
        let _mounts = iomount::test_hold_body(mounts(&remote, kind));
        let size = walk_until(&remote, deadline());
        assert!(!size.partial, "{kind}: local folder sizing stays enabled");
        assert!(size.bytes > own);
    }
}

#[test]
fn gh368_size_sort_of_home_skips_only_the_remote_row() {
    let d = TestDir::new("gh368-sort");
    let remote = d.dir("One Drive");
    d.file(
        "One Drive/remote-file",
        "remote contents must never be counted",
    );
    let local = d.dir("One Drive-local");
    d.file("One Drive-local/local-file", "local contents still count");
    d.file("plain-file", "file row");
    let mut listing = Listing::new();
    listing.push("One Drive", true);
    listing.push("One Drive-local", true);
    listing.push("plain-file", false);
    let (stats, _) = stat_all(d.path(), &listing);
    let _mounts = iomount::test_hold_body(mounts(&remote, "fuse.rclone"));
    let sizes = sort_walk(d.path(), "", &listing, &stats, deadline());
    let remote_size = sizes[0].unwrap();
    assert!(
        remote_size.partial,
        "a remote row under a local base is still remote"
    );
    assert_eq!(remote_size.bytes, stats[0].size);
    let local_size = sizes[1].unwrap();
    assert!(!local_size.partial);
    assert_eq!(local_size.bytes, walk_while(&local, &|| false).bytes);
    assert!(sizes[2].is_none());
}

#[test]
fn gh368_phone_sizes_and_cancellation_keep_their_guards() {
    struct RunUser;
    impl Drop for RunUser {
        fn drop(&mut self) {
            crate::backend::gvfsroot::set_test_run_user(None);
        }
    }
    let d = TestDir::new("gh368-phone");
    let phone = d.dir("1000/gvfs/mtp:host=phone");
    d.file(
        "1000/gvfs/mtp:host=phone/image.jpg",
        "phone contents must never be counted",
    );
    crate::backend::gvfsroot::set_test_run_user(Some(d.path()));
    let _run_user = RunUser;
    let _mounts = iomount::test_hold_body(String::new());
    let size = walk_until(&phone, deadline());
    assert!(size.partial);
    assert_eq!(size.bytes, phone.symlink_metadata().unwrap().size());
    let cancelled = walk_cancellable(&phone, deadline(), &|| true);
    assert!(cancelled.partial);
    assert_eq!(cancelled.bytes, 0);
}

#[test]
fn gh368_a_pane_reached_through_a_symlink_still_refuses_its_remote_row() {
    let d = TestDir::new("gh368-link");
    let remote = d.dir("mnt/onedrive");
    d.file("mnt/onedrive/remote-file", "remote contents must never be counted");
    let pane = d.join("cloud");
    std::os::unix::fs::symlink(d.path().join("mnt"), &pane).unwrap();
    let row = pane.join("onedrive");
    let own = remote.symlink_metadata().unwrap().size();
    let device = crate::backend::mountinfo::dev_majmin(remote.symlink_metadata().unwrap().dev());
    let root = "1 0 0:0 / / rw - ext4 /dev/root rw\n".to_string();
    let rclone = format!(
        "{root}2 1 {device} / /gh368-mounted-elsewhere rw - fuse.rclone remote: rw\n"
    );
    assert_eq!(
        crate::backend::mountinfo::mount_type_in(&row, &rclone).as_deref(),
        Some("ext4"),
        "the spelled path must miss the mount point"
    );
    {
        let _mounts = iomount::test_hold_body(root);
        let size = walk_until(&row, deadline());
        assert!(
            !size.partial && size.bytes > own,
            "without the device line the row is walked: {size:?}"
        );
    }
    let _mounts = iomount::test_hold_body(rclone);
    let size = walk_until(&row, deadline());
    assert!(size.partial, "a remote row reached through a symlinked pane is still remote");
    assert_eq!(size.bytes, own);
    let mut listing = Listing::new();
    listing.push("onedrive", true);
    let (stats, _) = stat_all(&pane, &listing);
    let sizes = sort_walk(&pane, "", &listing, &stats, deadline());
    assert!(sizes[0].unwrap().partial);
    assert_eq!(sizes[0].unwrap().bytes, stats[0].size);
}
