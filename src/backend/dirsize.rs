use crate::backend::extclass::refuses_folder_sizes;
use crate::backend::listing::Listing;
use crate::backend::meta::Stat;
#[cfg(test)]
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::time::Duration;
use std::time::Instant;

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
    walked
        .get(row)
        .and_then(|w| *w)
        .map(|w| w.bytes)
        .unwrap_or(0)
}

// One shared deadline bounds the whole folder pass; non-directory rows stay None.
pub fn walk_all(base: &Path, l: &Listing, deadline: Instant) -> Vec<Option<DirSize>> {
    let n = l.len();
    let mut out = vec![None; n];
    let mounts = super::iomount::mount_body();
    let mounts = mounts.as_str();
    let workers = std::thread::available_parallelism()
        .map(|w| w.get())
        .unwrap_or(1);
    // Ceiling division, so every row lands in exactly one chunk; max(1) keeps chunks_mut off zero.
    let per_worker = n.div_ceil(workers).max(1);
    std::thread::scope(|s| {
        for (k, slots) in out.chunks_mut(per_worker).enumerate() {
            let first = k * per_worker;
            s.spawn(move || {
                for (j, slot) in slots.iter_mut().enumerate() {
                    if l.is_dir(first + j) {
                        *slot = Some(walk_display(
                            &l.path(base, first + j),
                            &|| Instant::now() >= deadline,
                            mounts,
                        ));
                    }
                }
            });
        }
    });
    out
}

// On a share or a phone a size sort walks nothing: each folder keeps its entry size, partial, as an expired walk answers.
pub fn sort_walk(
    base: &Path,
    class: &str,
    l: &Listing,
    stats: &[Stat],
    deadline: Instant,
) -> Vec<Option<DirSize>> {
    if !refuses_folder_sizes(class) {
        return walk_all(base, l, deadline);
    }
    (0..l.len())
        .map(|i| {
            l.is_dir(i).then(|| DirSize {
                bytes: stats[i].size,
                partial: true,
            })
        })
        .collect()
}

// walk_until is the testable core: a test passes an already-past deadline to force partial without waiting 2000 ms.
pub fn walk(path: &Path) -> DirSize {
    walk_until(path, Instant::now() + Duration::from_millis(DEADLINE_MS))
}

pub fn walk_until(path: &Path, deadline: Instant) -> DirSize {
    walk_display(
        path,
        &|| Instant::now() >= deadline,
        &super::iomount::mount_body(),
    )
}

// The size worker walks through dirsizewatch::walk_watched; this unarmed form keeps the walk's own tests.
#[cfg(test)]
pub fn walk_cancellable(path: &Path, deadline: Instant, cancelled: &impl Fn() -> bool) -> DirSize {
    if cancelled() {
        return DirSize {
            bytes: 0,
            partial: true,
        };
    }
    walk_display(
        path,
        &|| cancelled() || Instant::now() >= deadline,
        &super::iomount::mount_body(),
    )
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
    arm: Arm<'_>,
) -> (DirSize, bool) {
    let by_path = super::mountinfo::mount_type_in(path, mounts);
    // NO_AUTOMOUNT protects only the basename, so refuse a known autofs ancestor before probing below it.
    if by_path.as_deref() == Some("autofs") {
        return (
            DirSize {
                bytes: 0,
                partial: true,
            },
            false,
        );
    }
    let entry = stat::at(path);
    // A symlinked pane can spell another path, so its row's device is checked beside its lexical mount.
    let kinds = [
        by_path,
        entry
            .as_ref()
            .and_then(|meta| super::mountinfo::fstype_for_dev(meta.dev, mounts)),
    ];
    let automount = entry.as_ref().is_some_and(|meta| meta.automount)
        || kinds.iter().flatten().any(|kind| kind == "autofs");
    let remote = super::extclass::gvfs_class(path).is_some_and(refuses_folder_sizes)
        || automount
        || kinds
            .iter()
            .flatten()
            .any(|kind| super::netfs::is_network_fstype(kind));
    if remote {
        return (
            DirSize {
                bytes: if automount {
                    0
                } else {
                    entry.map(|meta| meta.size).unwrap_or(0)
                },
                partial: true,
            },
            false,
        );
    }
    let blind = kinds
        .iter()
        .flatten()
        .any(|kind| super::netfs::inotify_blind(kind));
    let tally = traversal::walk_from(path, stop, None, None, arm.filter(|_| !blind), entry, false);
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
pub fn walk_counted<'a>(
    path: &Path,
    stop: &'a dyn Fn() -> bool,
    progress: Option<Progress<'a>>,
) -> Tally {
    walk_core(path, stop, None, progress)
}

// One lookup override for tests; None takes the entry's own device, so production pays no indirection.
pub(super) type DevLookup<'a> = Option<&'a dyn Fn(&Path) -> Option<u64>>;
pub(super) type Arm<'a> = Option<&'a dyn Fn(&Path, &Path)>;

#[path = "dirsizestat.rs"]
pub(super) mod stat;

#[path = "dirsizewalk.rs"]
mod traversal;
#[cfg(test)]
use traversal::same_device;
pub(super) use traversal::walk_core;

fn walk_while_with(path: &Path, stop: &dyn Fn() -> bool, dev_of: DevLookup<'_>) -> DirSize {
    let tally = walk_core(path, stop, dev_of, None);
    DirSize {
        bytes: tally.bytes,
        partial: tally.partial,
    }
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
#[path = "dirsize_mount_tests.rs"]
mod mount_tests;

#[cfg(test)]
#[path = "dirsize_tests.rs"]
mod tests;
