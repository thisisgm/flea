use crate::backend::extclass::refuses_folder_sizes;
use crate::backend::listing::Listing;
use crate::backend::meta::Stat;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::time::Instant;
use std::time::Duration;

// A directory over this deadline answers with what it saw, marked partial: a floor, not a wrong exact number.
pub(super) const DEADLINE_MS: u64 = 2000;
// corner: a size sort blocks the loop for this long at worst, so folders past it sort by their floor.
pub const SORT_BUDGET_MS: u64 = 250;

#[derive(Clone, Copy, Debug)]
pub struct DirSize {
    pub bytes: u64,
    pub partial: bool,
}

// A partial walk is a floor and sorts as one, with no special case in the comparison.
pub fn walked_bytes(walked: &[Option<DirSize>], row: usize) -> u64 {
    walked.get(row).and_then(|w| *w).map(|w| w.bytes).unwrap_or(0)
}

// One shared deadline bounds the whole folder pass; non-directory rows stay None.
pub fn walk_all(base: &Path, l: &Listing, deadline: Instant) -> Vec<Option<DirSize>> {
    let n = l.len();
    let mut out = vec![None; n];
    let mounts = super::iomount::mount_body();
    let mounts = mounts.as_str();
    let workers = std::thread::available_parallelism().map(|w| w.get()).unwrap_or(1);
    // Ceiling division, so every row lands in exactly one chunk; max(1) keeps chunks_mut off zero.
    let per_worker = n.div_ceil(workers).max(1);
    std::thread::scope(|s| {
        for (k, slots) in out.chunks_mut(per_worker).enumerate() {
            let first = k * per_worker;
            s.spawn(move || {
                for (j, slot) in slots.iter_mut().enumerate() {
                    if l.is_dir(first + j) {
                        *slot = Some(walk_display(&base.join(l.name(first + j)), &|| Instant::now() >= deadline, mounts));
                    }
                }
            });
        }
    });
    out
}

// On a share or a phone a size sort walks nothing: each folder keeps its entry size, partial, as an expired walk answers.
pub fn sort_walk(base: &Path, class: &str, l: &Listing, stats: &[Stat], deadline: Instant) -> Vec<Option<DirSize>> {
    if !refuses_folder_sizes(class) {
        return walk_all(base, l, deadline);
    }
    (0..l.len()).map(|i| l.is_dir(i).then(|| DirSize { bytes: stats[i].size, partial: true })).collect()
}

// walk_until is the testable core: a test passes an already-past deadline to force partial without waiting 2000 ms.
pub fn walk(path: &Path) -> DirSize {
    walk_until(path, Instant::now() + Duration::from_millis(DEADLINE_MS))
}

pub fn walk_until(path: &Path, deadline: Instant) -> DirSize {
    walk_display(path, &|| Instant::now() >= deadline, &super::iomount::mount_body())
}

// The size worker walks through dirsizewatch::walk_watched; this unarmed form keeps the walk's own tests.
#[cfg(test)]
pub fn walk_cancellable(path: &Path, deadline: Instant, cancelled: &impl Fn() -> bool) -> DirSize {
    if cancelled() { return DirSize { bytes: 0, partial: true }; }
    walk_display(path, &|| cancelled() || Instant::now() >= deadline, &super::iomount::mount_body())
}

// A row can itself be a remote mount under a local pane; the walk's device boundary cannot protect its starting folder.
fn walk_display(path: &Path, stop: &dyn Fn() -> bool, mounts: &str) -> DirSize {
    walk_display_armed(path, stop, mounts, None).0
}

// `arm` hears each folder before its read, never on a netfs::inotify_blind tree; the flag says a counted file may have a name outside it.
pub(super) fn walk_display_armed(
    path: &Path,
    stop: &dyn Fn() -> bool,
    mounts: &str,
    arm: Option<&dyn Fn(&Path)>,
) -> (DirSize, bool) {
    let network = |kind: String| super::netfs::is_network_fstype(&kind);
    let entry = path.symlink_metadata().ok();
    let remote = super::extclass::gvfs_class(path).is_some_and(refuses_folder_sizes)
        || super::mountinfo::mount_type_in(path, mounts).is_some_and(network)
        // A pane reached through a symlink never spells the mount point, so the row's own device names its mount too.
        || entry.as_ref().and_then(|meta| super::mountinfo::fstype_for_dev(meta.dev(), mounts)).is_some_and(network);
    if remote {
        return (DirSize { bytes: entry.map(|meta| meta.size()).unwrap_or(0), partial: true }, false);
    }
    let blind = || {
        let by_dev = entry
            .as_ref()
            .and_then(|meta| super::mountinfo::fstype_for_dev(meta.dev(), mounts));
        [super::mountinfo::mount_type_in(path, mounts), by_dev]
            .into_iter()
            .flatten()
            .any(|kind| super::netfs::inotify_blind(&kind))
    };
    let Some(arm) = arm.filter(|_| !blind()) else {
        return (walk_while(path, stop), false);
    };
    let mut quiet = |_: &Tally| {};
    let tally = walk_core(
        path,
        stop,
        None,
        Some(Progress {
            every: Duration::MAX,
            report: &mut quiet,
            entering: Some(arm),
        }),
    );
    (
        DirSize {
            bytes: tally.bytes,
            partial: tally.partial,
        },
        tally.linked,
    )
}

// Issue F1e: a copy's walk answers to the copy and not to a clock. The listing needs a floor inside
// two seconds; the transfer needs the whole total, however long the tree takes, or it draws no
// estimate at all, so the stop it is given is its own cancel rather than a deadline.
pub fn walk_while(path: &Path, stop: &dyn Fn() -> bool) -> DirSize {
    walk_while_with(path, stop, None)
}

// What a walk has counted so far: bytes include the start's own entry, items are the entries below it.
#[derive(Clone, Copy, Debug)]
pub struct Tally {
    pub bytes: u64,
    pub items: u64,
    pub partial: bool,
    // A counted regular file has a name outside the tree, or the walk could not tell, so a write through it may fire no watch here.
    pub linked: bool,
}

// The running tally goes to `report` at most once per `every`, never per entry; `entering` hears each folder before its listing.
pub struct Progress<'a> {
    pub every: Duration,
    pub report: &'a mut dyn FnMut(&Tally),
    pub entering: Option<&'a dyn Fn(&Path)>,
}

// The properties card counts a folder's contents with the same walk, answering to its own stop and progress.
pub fn walk_counted<'a>(path: &Path, stop: &'a dyn Fn() -> bool, progress: Option<Progress<'a>>) -> Tally {
    walk_core(path, stop, None, progress)
}

// One lookup override for tests; None takes the entry's own device, so production pays no indirection.
pub(super) type DevLookup<'a> = Option<&'a dyn Fn(&Path) -> Option<u64>>;

#[path = "dirsizewalk.rs"]
mod traversal;
pub(super) use traversal::walk_core;
#[cfg(test)]
use traversal::same_device;

fn walk_while_with(path: &Path, stop: &dyn Fn() -> bool, dev_of: DevLookup<'_>) -> DirSize {
    let tally = walk_core(path, stop, dev_of, None);
    DirSize { bytes: tally.bytes, partial: tally.partial }
}

#[cfg(test)]
#[path = "dirsize_race_tests.rs"]
mod race_tests;

#[cfg(test)]
#[path = "dirsize_gh368_tests.rs"]
mod gh368_tests;

#[cfg(test)]
#[path = "dirsize_blind_tests.rs"]
mod blind_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;
    use std::fs;
    use std::os::unix::fs::symlink;
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
            fs::read_dir("/proc/self/fd").unwrap()
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
        assert!(most.get() <= 2, "{} descriptors were open into the tree at once", most.get());
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
        assert!(result.bytes < 100_000, "the symlink's own small size counts, not the target it points at");
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
        assert!(!whole.partial, "a stop that never fires walks the tree whole");
        // Fires part way in, so the stop inside the loop is what ends it rather than the one on entry.
        let seen = std::sync::atomic::AtomicUsize::new(0);
        let cut = walk_while(&d, &|| seen.fetch_add(1, std::sync::atomic::Ordering::Relaxed) >= 3);
        assert!(cut.partial, "a stop that fires leaves a floor, which publishes no total at all");
        // The root's own entry is counted before the walk starts, so an entry seen is bytes above that.
        let root_only = fs::symlink_metadata(&d).unwrap().size();
        assert!(cut.bytes > root_only, "and it keeps what it saw before the stop: {} against {}", cut.bytes, root_only);
        assert!(cut.bytes < whole.bytes, "which is less than the whole tree: {} against {}", cut.bytes, whole.bytes);
        // The other entry point on the same tree, whose clock still bounds it where the copy's has none.
        let past = Instant::now() - Duration::from_secs(1);
        assert!(walk_until(&d, past).partial, "the listing's own deadline still bounds it");
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
        assert!(result.partial, "a subtree it could not read must mark partial");
        let expected_min = fs::symlink_metadata(&d).unwrap().size()
            + fs::symlink_metadata(d.join("visible.txt")).unwrap().size();
        assert!(result.bytes >= expected_min, "what the walk could see must still be counted");
    }

    #[test]
    fn a_walk_never_descends_off_its_starting_device() {
        assert!(same_device(Some(2049), 2049), "the starting device itself descends");
        assert!(!same_device(Some(2049), 62), "a nested tmpfs mount never descends");
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
        let out = fs::symlink_metadata(d.join("other/out.txt")).unwrap().size();
        assert_eq!(result.bytes, want, "the other-device subtree never descends, dropping the gate goes red");
        assert!(out > 0, "the skipped payload is real bytes, so the equality above is not vacuous");
    }

    #[test]
    fn a_missing_directory_answers_zero_and_partial_rather_than_a_panic() {
        let result = walk(Path::new("/definitely/not/here/flea-dirsize-test"));
        assert_eq!(result.bytes, 0);
        assert!(result.partial);
    }
}
