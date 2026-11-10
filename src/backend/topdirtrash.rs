// The topdir Trash handles subvolume and bind mounts that gio refuses without copying between devices.
use crate::backend::hometrash::{info_name, percent_encode, reserve};
use crate::backend::mountinfo;
use crate::backend::trash::Entry;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, MetadataExt};
use std::path::{Path, PathBuf};

mod locations;
mod records;
pub(crate) use records::{list, resolve, resolver, Record, PREFIX};

pub(crate) fn roots() -> Vec<PathBuf> {
    locations::known()
        .into_iter()
        .map(|location| location.root)
        .collect()
}

// One trash request reads the mount table once, checks each parent folder once and decides durability per mount.
#[derive(Default)]
pub(crate) struct Batch {
    mounts: Vec<(PathBuf, PathBuf, bool)>,
    canonical: std::collections::HashMap<PathBuf, bool>,
    durable: std::collections::HashMap<PathBuf, bool>,
}

impl Batch {
    pub(crate) fn read() -> Self {
        Self {
            mounts: mountinfo::mount_roots_in(&mount_body()),
            ..Self::default()
        }
    }
}

// The home filesystem keeps its fast path; mount roots and paths through aliases stay with gio.
fn topdir_root_for(batch: &mut Batch, original: &Path) -> Option<PathBuf> {
    original.file_name()?;
    if !original.is_absolute()
        || original.components().any(|part| {
            matches!(
                part,
                std::path::Component::ParentDir | std::path::Component::CurDir
            )
        })
    {
        return None;
    }
    let parent = original.parent()?;
    let canonical = *batch
        .canonical
        .entry(parent.to_path_buf())
        .or_insert_with(|| parent.canonicalize().is_ok_and(|real| real == parent));
    if !canonical {
        return None;
    }
    let (mount, _, served) = mountinfo::deepest(original, &batch.mounts)?;
    if !served || original == mount {
        return None;
    }
    let uid = crate::backend::manifestdir::current_uid();
    if original.starts_with(mount.join(".Trash"))
        || original.starts_with(mount.join(format!(".Trash-{uid}")))
    {
        return None;
    }
    Some(mount.clone())
}

#[cfg(test)]
thread_local! {
    static MOUNT_BODY: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}
#[cfg(test)]
pub(crate) fn test_set_mount_body(body: Option<String>) {
    MOUNT_BODY.with(|slot| *slot.borrow_mut() = body);
}
fn mount_body() -> String {
    #[cfg(test)]
    return MOUNT_BODY
        .with(|slot| slot.borrow().clone())
        .unwrap_or_default();
    #[cfg(not(test))]
    mountinfo::read_body()
}

// Classification is read-only; source validation happens before directories or records are created.
pub(crate) fn renames_to_topdir(batch: &mut Batch, original: &Path) -> bool {
    !crate::backend::hometrash::renames_to_home(original)
        && topdir_root_for(batch, original).is_some()
}

pub(crate) fn rename_to_topdir(
    batch: &mut Batch,
    original: &Path,
    unsynced: &mut std::collections::BTreeSet<PathBuf>,
) -> Option<Result<Entry, String>> {
    let top = topdir_root_for(batch, original)?;
    let name = original.file_name()?;
    let relative = original.strip_prefix(&top).ok()?;
    let confirm = *batch
        .durable
        .entry(top.clone())
        .or_insert_with(|| crate::backend::durable::dest_is_durable(&top));
    for location in locations::at(&top, true) {
        let files = locations::fd_path(&location.files);
        let info = locations::fd_path(&location.info);
        let Ok((candidate, info_path)) = reserve(&files, &info, relative, name, confirm) else {
            continue;
        };
        if let Err(error) =
            crate::backend::renamecompat::rename_noreplace(original, &files.join(&candidate))
        {
            let _ = std::fs::remove_file(&info_path);
            if error.kind() == std::io::ErrorKind::NotFound && original.symlink_metadata().is_err()
            {
                return Some(Err(crate::backend::collide::TRASH_REFUSED.to_string()));
            }
            continue;
        }
        let files = location.root.join("files");
        let info = location.root.join("info");
        let uri = records::uri(&files.join(&candidate));
        unsynced.extend(
            [files, info]
                .into_iter()
                .chain(original.parent().map(Path::to_path_buf)),
        );
        return Some(Ok(Entry {
            original: original.to_path_buf(),
            uri,
            before: Vec::new(),
            trashed_at: String::new(),
        }));
    }
    None
}

// Undo shares the Trash view's checked claim and record validation, including relative and absolute Path records.
pub(crate) fn restore_from_topdir(entry: &Entry) -> Option<Result<(), String>> {
    if !entry.uri.starts_with(PREFIX) {
        return None;
    }
    Some((|| {
        let record = resolve(&entry.uri)
            .ok_or_else(|| "the Trash entry could not be verified".to_string())?;
        if record.entry.original != entry.original {
            return Err("the Trash entry's original location changed".into());
        }
        if entry.original.symlink_metadata().is_ok() {
            return Err(crate::backend::homerestore::RESTORE_TAKEN.into());
        }
        #[cfg(not(test))]
        let recovery = crate::backend::trashdelete::recovery_root()?;
        #[cfg(test)]
        let recovery = {
            let top = topdir_root_for(&mut Batch::read(), &entry.original)
                .ok_or("test mount is missing")?;
            if !top.ancestors().any(crate::backend::testdir::removable) {
                return Err("test mount is not sandboxed".into());
            }
            top.join(".flea-test-recovery")
        };
        record
            .backing
            .restore(&entry.original, &recovery)
            .map_err(String::from)
    })())
}

#[cfg(test)]
mod integration_tests;
#[cfg(test)]
mod tests;
