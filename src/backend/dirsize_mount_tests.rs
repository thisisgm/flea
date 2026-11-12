use super::*;
use crate::backend::{iomount, testdir::TestDir};
use std::cell::Cell;

#[test]
fn gh458_an_untriggered_automount_row_is_never_opened_or_watched() {
    let d = TestDir::new("gh458-autofs");
    let sleeping = d.dir("sleeping share");
    d.file("sleeping share/payload", &"x".repeat(100_000));
    let escaped = sleeping.to_string_lossy().replace(' ', "\\040");
    let mounts = format!(
        "1 0 8:1 / / rw - ext4 /dev/root rw\n2 1 0:42 / {escaped} rw - autofs systemd-1 rw\n"
    );
    let armed = Cell::new(0);
    let arm = |_: &Path, _: &Path| armed.set(armed.get() + 1);
    let (size, _) = walk_display_armed(&sleeping, &|| false, &mounts, Some(&arm));
    assert!(
        size.partial,
        "an automount cannot publish a recursive total: {size:?}"
    );
    assert_eq!(size.bytes, 0, "an unmounted row has no size to display");
    assert_eq!(
        armed.get(),
        0,
        "watching the automount would also trigger it"
    );
    let _mounts = iomount::test_hold_body(mounts);
    assert!(walk_until(&sleeping, Instant::now() + Duration::from_secs(10)).partial);
}

#[test]
fn gh458_the_kernel_automount_attribute_alone_refuses_a_row() {
    let d = TestDir::new("gh458-autofs-attribute");
    let sleeping = d.dir("sleeping");
    d.file("sleeping/payload", &"x".repeat(100_000));
    let _fake = stat::fake::mount(&sleeping, true, None);
    let (size, _) = walk_display_armed(
        &sleeping,
        &|| false,
        "",
        Some(&|_, _| panic!("automount watched")),
    );
    assert_eq!(size.bytes, 0);
    assert!(size.partial);
}

#[test]
fn gh458_a_walk_skips_nested_automounts_and_same_device_bind_mounts() {
    for automount in [true, false] {
        let d = TestDir::new("gh458-nested");
        let root = d.dir("root");
        let child = d.dir("root/mounted");
        d.file("root/mounted/payload", &"x".repeat(100_000));
        d.file("root/local", "local");
        let expected = walk_while(&root, &|| false).bytes - 100_000;
        let start = stat::at(&root).unwrap();
        let child_entry = stat::at(&child).unwrap();
        assert_eq!(
            start.dev, child_entry.dev,
            "the fixture must exercise the same-device boundary"
        );
        // Each pass pins one branch: the automount flag on the same mount, or a new mount ID alone.
        let id = (!automount).then(|| start.mount.unwrap() + 1);
        let _fake = stat::fake::mount(&child, automount, id);
        let armed_child = Cell::new(false);
        let arm = |at: &Path, _: &Path| {
            if at == child {
                armed_child.set(true)
            }
        };
        let (size, _) = walk_display_armed(&root, &|| false, "", Some(&arm));
        assert_eq!(
            size.bytes, expected,
            "the mounted payload was read: {size:?}"
        );
        // A listing-time skip stays exact; only a refused open after listing would mark it partial.
        assert!(
            !size.partial,
            "the mount was not skipped at listing: {size:?}"
        );
        assert!(!armed_child.get(), "the skipped mount must not be watched");
    }
}

#[test]
fn gh458_a_pane_reached_through_a_symlink_still_refuses_autofs() {
    let d = TestDir::new("gh458-link");
    let sleeping = d.dir("parent/sleeping");
    d.file("parent/sleeping/payload", &"x".repeat(100_000));
    let pane = d.join("alias");
    std::os::unix::fs::symlink(d.join("parent"), &pane).unwrap();
    let dev = crate::backend::mountinfo::dev_majmin(sleeping.symlink_metadata().unwrap().dev());
    let mounts = format!("2 1 {dev} / /unrelated-spelling rw - autofs systemd-1 rw\n");
    let (size, _) = walk_display_armed(&pane.join("sleeping"), &|| false, &mounts, None);
    assert_eq!(size.bytes, 0);
    assert!(size.partial);
}

#[test]
fn gh458_a_size_sort_skips_the_automount_row_and_still_sizes_local_rows() {
    let d = TestDir::new("gh458-sort");
    let sleeping = d.dir("sleeping");
    let local = d.dir("local");
    d.file("sleeping/payload", &"x".repeat(100_000));
    d.file("local/payload", "local");
    let mut listing = Listing::new();
    listing.push("sleeping", true);
    listing.push("local", true);
    let (stats, _) = crate::backend::meta::stat_all(d.path(), &listing);
    let _mounts = iomount::test_hold_body(format!(
        "2 1 0:42 / {} rw - autofs systemd-1 rw\n",
        sleeping.display()
    ));
    let sizes = sort_walk(
        d.path(),
        "",
        &listing,
        &stats,
        Instant::now() + Duration::from_secs(10),
    );
    assert_eq!(sizes[0].unwrap().bytes, 0);
    assert!(sizes[0].unwrap().partial);
    assert_eq!(sizes[1].unwrap().bytes, walk_while(&local, &|| false).bytes);
    assert!(!sizes[1].unwrap().partial);
}

#[test]
fn gh458_a_watch_uses_the_open_inode_even_if_its_original_name_is_replaced() {
    let d = TestDir::new("gh458-held-watch");
    let root = d.dir("root");
    d.file("root/payload", "local");
    let outside = d.dir("outside");
    d.file("outside/remote-payload", &"x".repeat(100_000));
    let expected = stat::at(&root).unwrap().identity();
    let called = Cell::new(false);
    let arm = |logical: &Path, held: &Path| {
        assert_eq!(logical, root);
        std::fs::rename(&root, d.join("saved")).unwrap();
        std::os::unix::fs::symlink(&outside, &root).unwrap();
        assert_eq!(stat::at(held).unwrap().identity(), expected);
        called.set(true);
    };
    let (size, _) = walk_display_armed(&root, &|| false, "", Some(&arm));
    assert!(called.get());
    assert!(
        size.partial,
        "the changed original name cannot vouch for an exact total"
    );
    assert!(
        size.bytes < 100_000,
        "neither the watch nor the walk should follow the replacement"
    );
}
