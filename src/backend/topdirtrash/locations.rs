// Mount Trash directories are opened without following links and held while their entries are used.
use super::*;
use std::fs::{File, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;

// S_ISVTX: the Trash specification requires it on an administrator's $topdir/.Trash.
const STICKY_BIT: u32 = 0o1000;

pub(super) struct Location {
    pub top: PathBuf,
    pub root: PathBuf,
    pub files: File,
    pub info: File,
}

pub(super) fn fd_path(dir: &File) -> PathBuf {
    PathBuf::from(format!("/proc/self/fd/{}", dir.as_raw_fd()))
}

fn open(path: &Path, private: bool) -> Option<File> {
    let dir = OpenOptions::new()
        .read(true)
        .custom_flags(crate::oflags::O_NOFOLLOW | crate::oflags::O_DIRECTORY)
        .open(path)
        .ok()?;
    let meta = dir.metadata().ok()?;
    (!private
        || (meta.uid() == crate::backend::manifestdir::current_uid() && meta.mode() & 0o077 == 0))
        .then_some(dir)
}

fn child(parent: &File, name: &OsStr, create: bool) -> Option<File> {
    let path = fd_path(parent).join(name);
    if create {
        match std::fs::DirBuilder::new().mode(0o700).create(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return None,
        }
    }
    let dir = open(&path, true)?;
    (dir.metadata().ok()?.dev() == parent.metadata().ok()?.dev()).then_some(dir)
}

// Read-only discovery never creates a Trash; moves try the administrator's directory first.
pub(super) fn at(top: &Path, create: bool) -> Vec<Location> {
    let Some(volume) = open(top, false) else {
        return Vec::new();
    };
    let uid = crate::backend::manifestdir::current_uid();
    let mut candidates = Vec::new();
    if let Some(admin) = open(&fd_path(&volume).join(".Trash"), false) {
        if admin.metadata().is_ok_and(|meta| {
            meta.mode() & STICKY_BIT != 0
                && volume.metadata().is_ok_and(|top| top.dev() == meta.dev())
        }) {
            if let Some(shared) = child(&admin, OsStr::new(&uid.to_string()), create) {
                candidates.push((top.join(".Trash").join(uid.to_string()), shared));
            }
        }
    }
    let name = format!(".Trash-{uid}");
    if let Some(own) = child(&volume, OsStr::new(&name), create) {
        candidates.push((top.join(name), own));
    }
    candidates
        .into_iter()
        .filter_map(|(root, dir)| {
            Some(Location {
                top: top.to_path_buf(),
                root,
                files: child(&dir, OsStr::new("files"), create)?,
                info: child(&dir, OsStr::new("info"), create)?,
            })
        })
        .collect()
}

// Only mounts a move could have used are opened, so a stalled network mount or an automount point is never touched.
pub(super) fn known() -> Vec<Location> {
    let mut tops = std::collections::BTreeSet::new();
    for (mount, _, served) in mountinfo::mount_roots_in(&mount_body()) {
        if served {
            tops.insert(mount);
        }
    }
    tops.into_iter().flat_map(|top| at(&top, false)).collect()
}
