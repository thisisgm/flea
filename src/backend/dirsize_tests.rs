use super::*;
use crate::backend::testdir::TestDir;
use std::fs;
use std::os::unix::fs::symlink;
use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::PermissionsExt;

fn fixture(tag: &str) -> (TestDir, std::path::PathBuf) {
    let sandbox = TestDir::new(tag);
    let tree = sandbox.dir("tree");
    (sandbox, tree)
}

// Counted inside the walk, so it pins at most two tree descriptors regardless of depth or RLIMIT_NOFILE.
#[test]
fn a_deep_walk_holds_at_most_two_descriptors_open_into_the_tree() {
    let (_d, tree) = fixture("dirsize-descriptors");
    let mut deepest = tree.clone();
    for _ in 0..40 {
        deepest.push("d");
        fs::create_dir(&deepest).unwrap();
    }
    let open_into_tree = || {
        fs::read_dir("/proc/self/fd")
            .unwrap()
            .filter_map(|fd| fs::read_link(fd.ok()?.path()).ok())
            .filter(|target| target.starts_with(&tree))
            .count()
    };
    let most = std::cell::Cell::new(0);
    let stop = || {
        most.set(most.get().max(open_into_tree()));
        false
    };
    let size = walk_while(&tree, &stop);
    assert!(!size.partial, "the walk reached the bottom");
    assert!(
        most.get() <= 2,
        "{} descriptors were open into the tree at once",
        most.get()
    );
}

#[test]
fn cancellation_is_checked_again_during_traversal() {
    let d = TestDir::new("dirsize-cancel-midwalk");
    d.file("payload", "not counted");
    let calls = std::cell::Cell::new(0);
    let result = walk_cancellable(d.path(), Instant::now() + Duration::from_secs(2), &|| {
        calls.set(calls.get() + 1);
        calls.get() >= 3
    });
    assert!(result.partial);
    assert_eq!(result.bytes, d.path().symlink_metadata().unwrap().size());
}

#[test]
fn cancellation_stops_a_walk_before_reading_the_directory() {
    let d = TestDir::new("dirsize-cancel");
    d.file("payload", "not counted");
    let result = walk_cancellable(d.path(), Instant::now() + Duration::from_secs(2), &|| true);
    assert!(result.partial);
    assert_eq!(result.bytes, 0);
}

#[test]
fn an_empty_directory_counts_only_its_own_entry() {
    let (_sandbox, d) = fixture("dirsize-empty");
    let result = walk(&d);
    let own = fs::symlink_metadata(&d).unwrap().size();
    assert_eq!(result.bytes, own);
    assert!(!result.partial);
}

#[test]
fn files_and_nested_directories_sum_together() {
    let (_sandbox, d) = fixture("dirsize-nested");
    fs::write(d.join("a.txt"), "abc").unwrap();
    fs::create_dir(d.join("sub")).unwrap();
    fs::write(d.join("sub/b.txt"), "de").unwrap();
    let result = walk(&d);
    let expected = fs::symlink_metadata(&d).unwrap().size()
        + fs::symlink_metadata(d.join("a.txt")).unwrap().size()
        + fs::symlink_metadata(d.join("sub")).unwrap().size()
        + fs::symlink_metadata(d.join("sub/b.txt")).unwrap().size();
    assert_eq!(result.bytes, expected);
    assert!(!result.partial);
}

#[test]
fn a_symlink_is_not_followed() {
    // The target lives outside the walked tree, so a followed link is the only way its 100,000 bytes could ever show up.
    let (_sandbox, d) = fixture("dirsize-symlink");
    let (_target_sandbox, target) = fixture("dirsize-symlink-target");
    fs::write(target.join("huge.bin"), vec![0u8; 100_000]).unwrap();
    symlink(&target, d.join("link")).unwrap();
    let result = walk(&d);
    let expected = fs::symlink_metadata(&d).unwrap().size()
        + fs::symlink_metadata(d.join("link")).unwrap().size();
    assert_eq!(result.bytes, expected);
    assert!(
        result.bytes < 100_000,
        "the symlink's own small size counts, not the target it points at"
    );
}

#[test]
fn an_expired_deadline_marks_partial_and_keeps_what_it_saw() {
    let (_sandbox, d) = fixture("dirsize-deadline");
    fs::write(d.join("a.txt"), "abc").unwrap();
    let past = Instant::now() - Duration::from_secs(1);
    let result = walk_until(&d, past);
    assert!(result.partial);
}

// Directive 50: a copy's walk carries no deadline, so what ends it early is its own stop, which
// the transfer sets on a cancel and on its own completion.
#[test]
fn a_walk_ends_on_its_stop_rather_than_on_a_clock() {
    let (_sandbox, d) = fixture("dirsize-stop");
    // Flat, so the stop fires between two entries rather than on the way into another directory.
    for name in ["a", "b", "c", "d", "e", "f"] {
        fs::write(d.join(name), "abcdefgh").unwrap();
    }
    fs::create_dir(d.join("inner")).unwrap();
    fs::write(d.join("inner/g"), "abcdefgh").unwrap();
    let whole = walk_while(&d, &|| false);
    assert!(
        !whole.partial,
        "a stop that never fires walks the tree whole"
    );
    // Fires part way in, so the stop inside the loop is what ends it rather than the one on entry.
    let seen = std::sync::atomic::AtomicUsize::new(0);
    let cut = walk_while(&d, &|| {
        seen.fetch_add(1, std::sync::atomic::Ordering::Relaxed) >= 3
    });
    assert!(
        cut.partial,
        "a stop that fires leaves a floor, which publishes no total at all"
    );
    // The root's own entry is counted before the walk starts, so an entry seen is bytes above that.
    let root_only = fs::symlink_metadata(&d).unwrap().size();
    assert!(
        cut.bytes > root_only,
        "and it keeps what it saw before the stop: {} against {}",
        cut.bytes,
        root_only
    );
    assert!(
        cut.bytes < whole.bytes,
        "which is less than the whole tree: {} against {}",
        cut.bytes,
        whole.bytes
    );
    // The other entry point on the same tree, whose clock still bounds it where the copy's has none.
    let past = Instant::now() - Duration::from_secs(1);
    assert!(
        walk_until(&d, past).partial,
        "the listing's own deadline still bounds it"
    );
}

#[test]
fn a_permission_denied_subtree_adds_what_it_saw_and_marks_partial() {
    let (_sandbox, d) = fixture("dirsize-denied");
    fs::write(d.join("visible.txt"), "abc").unwrap();
    fs::create_dir(d.join("locked")).unwrap();
    fs::write(d.join("locked/hidden.txt"), "xyz").unwrap();
    fs::set_permissions(d.join("locked"), fs::Permissions::from_mode(0o000)).unwrap();
    let result = walk(&d);
    fs::set_permissions(d.join("locked"), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        result.partial,
        "a subtree it could not read must mark partial"
    );
    let expected_min = fs::symlink_metadata(&d).unwrap().size()
        + fs::symlink_metadata(d.join("visible.txt")).unwrap().size();
    assert!(
        result.bytes >= expected_min,
        "what the walk could see must still be counted"
    );
}

#[test]
fn a_walk_never_descends_off_its_starting_device() {
    assert!(
        same_device(Some(2049), 2049),
        "the starting device itself descends"
    );
    assert!(
        !same_device(Some(2049), 62),
        "a nested tmpfs mount never descends"
    );
    assert!(same_device(None, 62), "an unknown start walks as before");
}

#[test]
fn a_walk_applies_its_device_gate_to_every_folder_it_lists() {
    // Sample input: dev_of answers another device for "other", so its payload stays uncounted.
    let (_sandbox, d) = fixture("dirsize-device-gate");
    fs::create_dir(d.join("keep")).unwrap();
    fs::write(d.join("keep/in.txt"), "12345678").unwrap();
    fs::create_dir(d.join("other")).unwrap();
    fs::write(d.join("other/out.txt"), "12345678").unwrap();
    let start_dev = fs::symlink_metadata(&d).unwrap().dev();
    let other = d.join("other");
    let dev_of = |p: &Path| {
        if p.starts_with(&other) || *p == other {
            Some(start_dev.wrapping_add(1))
        } else {
            None
        }
    };
    let result = walk_while_with(&d, &|| false, Some(&dev_of));
    assert!(!result.partial, "the gate skips rather than failing");
    let want = fs::symlink_metadata(&d).unwrap().size()
        + fs::symlink_metadata(d.join("keep")).unwrap().size()
        + fs::symlink_metadata(d.join("keep/in.txt")).unwrap().size()
        + fs::symlink_metadata(d.join("other")).unwrap().size();
    let out = fs::symlink_metadata(d.join("other/out.txt"))
        .unwrap()
        .size();
    assert_eq!(
        result.bytes, want,
        "the other-device subtree never descends, dropping the gate goes red"
    );
    assert!(
        out > 0,
        "the skipped payload is real bytes, so the equality above is not vacuous"
    );
}

#[test]
fn a_missing_directory_answers_zero_and_partial_rather_than_a_panic() {
    let result = walk(Path::new("/definitely/not/here/flea-dirsize-test"));
    assert_eq!(result.bytes, 0);
    assert!(result.partial);
}
