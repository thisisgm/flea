use super::*;

#[test]
fn review_constructor_handle_open_failure_cleans_its_created_root() {
    const CHILD_ROOT: &str = "FLEA_TEST_HANDLE_OPEN_FAILURE_ROOT";
    if let Some(root) = std::env::var_os(CHILD_ROOT) {
        let root = PathBuf::from(root);
        assert!(root.is_absolute() && !root.as_os_str().is_empty());
        assert!(root.join(MARKER).is_file());
        #[repr(C)]
        struct Limit { current: u64, maximum: u64 }
        extern "C" {
            fn setrlimit(resource: i32, limit: *const Limit) -> i32;
        }
        assert_eq!(unsafe { setrlimit(7, &Limit { current: 0, maximum: 0 }) }, 0);
        let result = std::panic::catch_unwind(|| TestDir::new_in("handle-open-failure", &root));
        assert!(result.is_err(), "the held-directory open must fail without descriptor capacity");
        return;
    }
    let outer = TestDir::new("handle-open-failure");
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "backend::testdir::tests::review_constructor_handle_open_failure_cleans_its_created_root", "--nocapture"])
        .env(CHILD_ROOT, outer.path())
        .stdin(std::process::Stdio::null()).output().unwrap();
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(result.status.success(), "{stderr}");
    assert!(stderr.contains("hold the newly created test sandbox") && stderr.contains("Too many open files"), "{stderr}");
    let leaked: Vec<_> = std::fs::read_dir(outer.path()).unwrap().map(|entry| entry.unwrap())
        .filter(|entry| entry.file_type().unwrap().is_dir()).map(|entry| entry.path()).collect();
    assert!(leaked.is_empty(), "constructor leaked roots after directory-handle open failure: {leaked:?}");
}

#[test]
fn review_rollback_keeps_a_different_directory_at_its_old_name() {
    let outer = TestDir::new("rollback-replaced");
    let path = outer.join("flea-test-constructing");
    outer.assert_contains(&path);
    std::fs::create_dir(&path).unwrap();
    outer.file("flea-test-constructing/.flea-test-sandbox", "original marker");
    let rollback = UnmarkedDir::new(&path).unwrap();
    let held = outer.join("held-original");
    std::fs::rename(&path, &held).unwrap();
    std::fs::create_dir(&path).unwrap();
    let replacement = path.symlink_metadata().unwrap();
    drop(rollback);
    assert!(path.is_dir(), "rollback deleted a replacement directory it never created");
    let after = path.symlink_metadata().unwrap();
    assert_eq!((after.dev(), after.ino()), (replacement.dev(), replacement.ino()));
    assert_eq!(std::fs::read_to_string(held.join(MARKER)).unwrap(), "original marker", "a mismatch leaves the original alone too");
}

#[test]
fn review_rollback_keeps_a_marker_through_a_replacement_symlink() {
    let outer = TestDir::new("rollback-symlink");
    let victim = outer.dir("other-root");
    let marker = outer.file("other-root/.flea-test-sandbox", "other root marker");
    let sentinel = outer.file("other-root/keep.txt", "must survive");
    let path = outer.join("flea-test-constructing");
    outer.assert_contains(&path);
    std::fs::create_dir(&path).unwrap();
    outer.file("flea-test-constructing/.flea-test-sandbox", "original marker");
    let rollback = UnmarkedDir::new(&path).unwrap();
    let held = outer.join("held-original");
    std::fs::rename(&path, &held).unwrap();
    std::os::unix::fs::symlink(&victim, &path).unwrap();
    drop(rollback);
    assert_eq!(std::fs::read_to_string(sentinel).unwrap(), "must survive");
    assert!(marker.is_file(), "rollback unlinked the marker of a different nonempty root through a symlink");
    assert_eq!(std::fs::read_link(&path).unwrap(), victim);
    assert_eq!(std::fs::read_to_string(marker).unwrap(), "other root marker");
    assert_eq!(std::fs::read_to_string(held.join(MARKER)).unwrap(), "original marker", "a mismatch leaves the original alone too");
}

#[test]
fn review_constructor_failure_cannot_unlink_another_root_marker() {
    const CHILD_ROOT: &str = "FLEA_TEST_CONSTRUCTOR_SWAP_ROOT";
    if let Some(root) = std::env::var_os(CHILD_ROOT) {
        let root = PathBuf::from(root);
        assert!(root.is_absolute() && !root.as_os_str().is_empty());
        assert!(root.join(MARKER).is_file());
        let tag = "marker-swap-child";
        let path = root.join(format!("{PREFIX}{tag}-{}-{}", std::process::id(), NEXT.load(Ordering::Relaxed)));
        let held = root.join("held-original");
        let victim = root.join("other-root");
        std::panic::set_hook(Box::new(move |_| {
            std::fs::rename(&path, &held).unwrap();
            std::os::unix::fs::symlink(&victim, &path).unwrap();
        }));
        #[repr(C)]
        struct Limit { current: u64, maximum: u64 }
        extern "C" {
            fn setrlimit(resource: i32, limit: *const Limit) -> i32;
            fn signal(number: i32, handler: usize) -> usize;
        }
        assert_ne!(unsafe { signal(25, 1) }, usize::MAX);
        assert_eq!(unsafe { setrlimit(1, &Limit { current: 0, maximum: 0 }) }, 0);
        let result = std::panic::catch_unwind(|| TestDir::new_in(tag, &root));
        assert!(result.is_err(), "the real constructor must hit the marker write failure");
        return;
    }
    let outer = TestDir::new("real-constructor-swap");
    outer.dir("other-root");
    let marker = outer.file("other-root/.flea-test-sandbox", "other root marker");
    let sentinel = outer.file("other-root/keep.txt", "must survive");
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "backend::testdir::tests::review_constructor_failure_cannot_unlink_another_root_marker", "--nocapture"])
        .env(CHILD_ROOT, outer.path())
        .stdin(std::process::Stdio::null()).output().unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    assert_eq!(std::fs::read_to_string(sentinel).unwrap(), "must survive");
    assert!(marker.is_file(), "real constructor rollback unlinked another root marker after a path swap");
    assert_eq!(std::fs::read_to_string(marker).unwrap(), "other root marker");
    assert_eq!(std::fs::metadata(outer.join("held-original").join(MARKER)).unwrap().len(), 0, "a mismatch preserves the partial original marker");
}

#[test]
fn freed_inode_marker_write_failure_cleans_created_root() {
    const CHILD_ROOT: &str = "FLEA_TEST_MARKER_FAILURE_ROOT";
    if let Some(root) = std::env::var_os(CHILD_ROOT) {
        const RLIMIT_FSIZE: i32 = 1;
        const SIGXFSZ: i32 = 25;
        const SIG_IGN: usize = 1;
        #[repr(C)]
        struct Limit { current: u64, maximum: u64 }
        extern "C" {
            fn setrlimit(resource: i32, limit: *const Limit) -> i32;
            fn signal(number: i32, handler: usize) -> usize;
        }
        assert_ne!(unsafe { signal(SIGXFSZ, SIG_IGN) }, usize::MAX, "ignore the file-size signal");
        assert_eq!(unsafe { setrlimit(RLIMIT_FSIZE, &Limit { current: 0, maximum: 0 }) }, 0);
        let result = std::panic::catch_unwind(|| TestDir::new_in("marker-write-failure", Path::new(&root)));
        assert!(result.is_err(), "the injected marker write must fail");
        return;
    }
    let outer = TestDir::new("marker-write-failure");
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "backend::testdir::tests::freed_inode_marker_write_failure_cleans_created_root", "--nocapture"])
        .env(CHILD_ROOT, outer.path())
        .stdin(std::process::Stdio::null()).output().unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let leaked: Vec<_> = std::fs::read_dir(outer.path()).unwrap().map(|entry| entry.unwrap())
        .filter(|entry| entry.file_type().unwrap().is_dir()).map(|entry| entry.path()).collect();
    assert!(leaked.is_empty(), "constructor leaked roots after marker failure: {leaked:?}");
}

#[test]
fn an_explicit_sandbox_root_keeps_containment_and_cleanup() {
    let outer = TestDir::new("explicit-root");
    let kept;
    {
        let inner = TestDir::new_in("nested", outer.path());
        inner.file("a.txt", "body");
        inner.assert_contains(&inner.join("a.txt"));
        assert!(std::panic::catch_unwind(|| inner.assert_contains(outer.path())).is_err());
        kept = inner.path().to_path_buf();
    }
    assert!(!kept.exists(), "an explicit-root sandbox is removed on drop");
    assert!(outer.path().is_dir(), "the parent sandbox stays in place");
    assert!(allowed_root(Path::new("relative")).is_none());
    assert!(allowed_root(&PathBuf::from(std::env::var_os("HOME").unwrap())).is_none());
}

// The repair for eleven leaked roots: without it this drop cannot unlink held.txt and the whole
// sandbox stays on disk. Reverting the retry in Drop reddens exactly this assertion.
#[test]
fn a_sandbox_holding_a_directory_the_owner_cannot_write_is_still_removed() {
    let kept = {
        let d = TestDir::new("dropreadonly");
        let sub = d.dir("locked");
        std::fs::write(sub.join("held.txt"), "x").unwrap();
        // 0500 cannot be written into and 0000 cannot even be opened, and a test may leave either.
        let shut = d.dir("shut");
        std::fs::write(shut.join("inside.txt"), "x").unwrap();
        std::fs::set_permissions(&shut, std::fs::Permissions::from_mode(0o000)).unwrap();
        std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o500)).unwrap();
        d.path().to_path_buf()
    };
    assert!(!kept.exists(), "the sandbox goes with the test that made it: {}", kept.display());
}

// The walk is handed a name that is a symlink, which is the shape a planted /tmp entry takes, and
// has to change nothing at the other end rather than chmodding its way down somebody else's tree.
#[test]
fn the_repair_walk_changes_nothing_through_a_symlink_it_is_handed() {
    let d = TestDir::new("dropsymlink");
    let target = d.dir("elsewhere");
    std::fs::write(target.join("inside.txt"), "x").unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o500)).unwrap();
    let planted = d.join("planted");
    std::os::unix::fs::symlink(&target, &planted).unwrap();
    owner_can_write(&planted);
    let mode = target.symlink_metadata().unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o500, "a symlink is not the directory to repair, and its target is untouched");
}

#[test]
fn temporary_roots_never_include_home_or_its_descendants() {
    let home = Path::new("/home/gm");
    assert!(canonical_temp_root_allowed(Path::new("/tmp"), home));
    assert!(canonical_temp_root_allowed(Path::new("/private/tmp"), Path::new("/Users/gm")));
    for path in ["/home/gm", "/home/gm/Work", "/home/gm/.config", "/home/gm/.local", "/home/gm/.cache", "/home/gm/tmp", "/home/gm/tmp/nested", "/", "", "relative"] {
        assert!(!canonical_temp_root_allowed(Path::new(path), home), "unsafe temporary root: {}", path);
    }
    assert!(!canonical_temp_root_allowed(Path::new("/tmp"), Path::new("relative")));
    assert!(!canonical_temp_root_allowed(Path::new("/tmp"), Path::new("/")));
}

#[test]
fn a_sandbox_carries_its_marker_and_is_removable() {
    let d = TestDir::new("guard");
    assert!(d.path().is_dir());
    assert!(d.path().join(MARKER).is_file());
    assert!(removable(d.path()));
}

#[test]
fn containment_refuses_empty_relative_outside_and_parent_paths() {
    let d = TestDir::new("containment");
    d.assert_contains(d.path());
    d.assert_contains(&d.join("missing/child"));
    for path in [PathBuf::new(), PathBuf::from("relative"), PathBuf::from("/"), d.join("../outside")] {
        assert!(std::panic::catch_unwind(|| d.assert_contains(&path)).is_err());
    }
}

#[test]
fn the_guard_refuses_every_path_that_is_not_one_of_ours() {
    // A real directory with no marker: the case the incident actually needed refused.
    let outside = TestDir::new("outside");
    let plain = outside.dir("payload");
    assert!(!removable(&plain));
    // The home directory and its parents, named explicitly because they are what was lost.
    assert!(!removable(Path::new("/home/gm")));
    assert!(!removable(Path::new("/home")));
    assert!(!removable(Path::new("/")));
    assert!(!removable(Path::new("")));
    // A relative path can be anything the caller's cwd makes it, so it never qualifies.
    assert!(!removable(Path::new("flea-test-relative")));
    // Right shape, right place, no marker.
    let bare = outside.dir(&format!("{}bare", PREFIX));
    assert!(!removable(&bare));
}

#[test]
fn a_replaced_path_reads_new_bytes_while_its_old_handle_keeps_old_bytes() {
    let d = TestDir::new("replacefile");
    let path = d.file("a.txt", "old");
    let mut held = std::fs::File::open(&path).unwrap();
    d.replace_file(&path, "new");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
    let mut old_body = String::new();
    std::io::Read::read_to_string(&mut held, &mut old_body).unwrap();
    assert_eq!(old_body, "old", "replacement must not rewrite the held file");
}

#[test]
fn replacing_an_unheld_file_changes_its_inode_without_birth_time() {
    // Repeated replacements exercise freed-inode reuse without waiting for a birth-time tick.
    const REPLACEMENTS: usize = 64;
    let d = TestDir::new("replaceidentity");
    let path = d.file("a.txt", "old");
    // replace_file asserts the new inode and the new body on every pass.
    for _ in 0..REPLACEMENTS {
        d.replace_file(&path, "new");
    }
}

#[test]
fn a_dropped_sandbox_takes_its_contents_with_it() {
    let kept;
    {
        let d = TestDir::new("drop");
        d.file("a.txt", "body");
        d.dir("sub");
        kept = d.path().to_path_buf();
        assert!(kept.is_dir());
    }
    assert!(!kept.exists());
}
