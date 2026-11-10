// Tree walks that keep their place on the heap and hold a bounded number of descriptors at any depth.
use super::{held_path, open_dir};
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

// The deepest sixteen levels keep their descriptors; a shallower one is reopened through "..", checked by identity.
pub(crate) const HELD_LEVELS: usize = 16;
const O_PATH: i32 = 0o10000000;
// access(2) modes, the sticky bit and EPERM, the same on x86_64 and aarch64.
const W_OK: i32 = 2;
const X_OK: i32 = 1;
const S_ISVTX: u32 = 0o1000;
const EPERM: i32 = 1;

extern "C" {
    fn access(path: *const std::ffi::c_char, mode: i32) -> i32;
    fn geteuid() -> u32;
}
// Linux PATH_MAX; a longer path is opened a stretch at a time through held descriptors.
const PATH_MAX: usize = 4096;
const STRETCH: usize = 2048;

pub(crate) type Identity = (u64, u64);

pub(crate) fn identity_of(file: &File) -> io::Result<Identity> {
    let meta = file.metadata()?;
    Ok((meta.dev(), meta.ino()))
}

// A directory reached through ".." that is not the one entered by name was moved meanwhile.
pub(crate) fn moved_meanwhile() -> io::Error {
    io::Error::other("a folder was moved while it was being read")
}

struct Level {
    name: OsString,
    id: Identity,
    fd: Option<File>,
}

struct Held {
    name: OsString,
    id: Identity,
    fd: File,
}

// A path of directories below a held base, each entered by name with O_NOFOLLOW and O_DIRECTORY.
pub(crate) struct Chain {
    base: File,
    top: Option<Held>,
    levels: Vec<Level>,
}

impl Chain {
    pub(crate) fn new(base: File) -> Chain {
        Chain {
            base,
            top: None,
            levels: Vec::new(),
        }
    }

    #[cfg(test)]
    pub(crate) fn depth(&self) -> usize {
        self.levels.len() + usize::from(self.top.is_some())
    }

    // The top level always holds its descriptor: push opens it and pop reopens it before anything reads it.
    pub(crate) fn top(&self) -> &File {
        self.top.as_ref().map_or(&self.base, |held| &held.fd)
    }

    // The folder the chain started from, for the work that follows the walk.
    pub(crate) fn into_base(self) -> File {
        self.base
    }

    pub(crate) fn top_name(&self) -> Option<&OsStr> {
        self.top.as_ref().map(|held| held.name.as_os_str())
    }

    // Takes a directory already opened below the top; answers the level whose descriptor was let go, if any.
    pub(crate) fn push_open(&mut self, name: &OsStr, dir: File) -> io::Result<Option<usize>> {
        let id = identity_of(&dir)?;
        let held = Held {
            name: name.to_os_string(),
            id,
            fd: dir,
        };
        if let Some(parent) = self.top.replace(held) {
            self.levels.push(Level {
                name: parent.name,
                id: parent.id,
                fd: Some(parent.fd),
            });
        }
        let Some(shallow) = self.levels.len().checked_sub(HELD_LEVELS) else {
            return Ok(None);
        };
        Ok(self.levels[shallow].fd.take().map(|_| shallow))
    }

    #[cfg(test)]
    pub(crate) fn push(&mut self, name: &OsStr) -> io::Result<Option<usize>> {
        let dir = open_dir(&held_path(self.top()).join(name))?;
        self.push_open(name, dir)
    }

    // Leaves the top level and hands back its descriptor; a parent that let its own go is reopened through "..".
    pub(crate) fn pop(&mut self) -> io::Result<File> {
        let no_child = || io::Error::new(io::ErrorKind::InvalidInput, "the chain is at its base");
        let child = self.top.as_ref().ok_or_else(no_child)?;
        let parent_fd = match self.levels.last_mut() {
            Some(parent) => Some(match parent.fd.take() {
                Some(dir) => dir,
                None => {
                    let dir = open_dir(&held_path(&child.fd).join(".."))?;
                    if identity_of(&dir)? != parent.id {
                        return Err(moved_meanwhile());
                    }
                    dir
                }
            }),
            None => None,
        };
        let parent = self.levels.pop().zip(parent_fd).map(|(level, fd)| Held {
            name: level.name,
            id: level.id,
            fd,
        });
        std::mem::replace(&mut self.top, parent)
            .map(|held| held.fd)
            .ok_or_else(no_child)
    }

    #[cfg(test)]
    pub(crate) fn held(&self) -> usize {
        usize::from(self.top.is_some())
            + self
                .levels
                .iter()
                .filter(|level| level.fd.is_some())
                .count()
    }
}

// `fix` lends a folder whose mode refuses removal the owner's bits, for a copy this run made; a move's source gets none.
pub(crate) fn remove_deep(path: &Path, fix: bool) -> io::Result<()> {
    remove_counting(path, fix, &mut 0)
}

// The same removal, counting what it removed, so a caller can tell a refusal before anything went from one partway.
pub(crate) fn remove_counting(path: &Path, fix: bool, gone: &mut usize) -> io::Result<()> {
    let meta = path.symlink_metadata()?;
    if !meta.is_dir() {
        return std::fs::remove_file(path);
    }
    let mut chain = Chain::new(open_fixing(path, fix)?);
    let mut pending = vec![clear(chain.top(), fix, gone)?];
    while let Some(folders) = pending.last_mut() {
        if let Some(name) = folders.pop() {
            let opened = open_fixing(&held_path(chain.top()).join(&name), fix);
            match opened {
                Ok(dir) => {
                    chain.push_open(&name, dir)?;
                    pending.push(clear(chain.top(), fix, gone)?);
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
            continue;
        }
        pending.pop();
        let Some(name) = chain.top_name().map(OsStr::to_os_string) else {
            break;
        };
        chain.pop()?;
        retry_fixed(chain.top(), fix, || {
            std::fs::remove_dir(held_path(chain.top()).join(&name))
        })?;
        *gone += 1;
    }
    drop(chain);
    std::fs::remove_dir(path)
}

// Removes everything in the folder that is not a folder and answers the folders, which the caller enters next.
fn clear(dir: &File, fix: bool, gone: &mut usize) -> io::Result<Vec<OsString>> {
    let at = held_path(dir);
    let mut folders = Vec::new();
    for entry in std::fs::read_dir(&at)? {
        let entry = entry?;
        let name = entry.file_name();
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => folders.push(name),
            Ok(_) => {
                retry_fixed(dir, fix, || std::fs::remove_file(at.join(&name)))?;
                *gone += 1;
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    Ok(folders)
}

// Something already gone counts as removed; a refusal from the folder's own mode is fixed once when allowed.
fn retry_fixed(dir: &File, fix: bool, remove: impl Fn() -> io::Result<()>) -> io::Result<()> {
    let refused = |e: &io::Error| fix && matches!(e.kind(), io::ErrorKind::PermissionDenied);
    match remove() {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) if refused(&e) => {
            std::fs::set_permissions(held_path(dir), std::fs::Permissions::from_mode(0o700))?;
            match remove() {
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
                other => other,
            }
        }
        Err(e) => Err(e),
    }
}

// A folder its own mode keeps closed takes the owner's bits through an O_PATH descriptor, so a symlink swapped in is never followed.
fn open_fixing(path: &Path, fix: bool) -> io::Result<File> {
    match open_dir(path) {
        Err(e) if fix && e.kind() == io::ErrorKind::PermissionDenied => {
            let handle = std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(O_PATH | crate::oflags::O_NOFOLLOW | crate::oflags::O_DIRECTORY)
                .open(path)?;
            std::fs::set_permissions(held_path(&handle), std::fs::Permissions::from_mode(0o700))?;
            // The magic link refuses O_NOFOLLOW, so the name is opened again and must still be the folder fixed.
            let dir = open_dir(path)?;
            if identity_of(&dir)? != identity_of(&handle)? {
                return Err(moved_meanwhile());
            }
            Ok(dir)
        }
        other => other,
    }
}

// A move may take `item` out only if the folder lets this process remove entries, and a sticky one owns or holds it.
pub(crate) fn removable_from(folder: &Path, item: &std::fs::Metadata) -> io::Result<()> {
    writable(folder)?;
    let me = unsafe { geteuid() };
    let meta = folder.metadata()?;
    if meta.mode() & S_ISVTX != 0 && me != 0 && me != meta.uid() && me != item.uid() {
        return Err(io::Error::from_raw_os_error(EPERM));
    }
    Ok(())
}

// Whether this process may add and remove entries in the folder; a read-only drive answers EROFS.
pub(crate) fn writable(folder: &Path) -> io::Result<()> {
    let at = std::ffi::CString::new(std::os::unix::ffi::OsStrExt::as_bytes(folder.as_os_str()))
        .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    if unsafe { access(at.as_ptr(), W_OK | X_OK) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

// Opens a folder whose path may be longer than PATH_MAX, a stretch of components at a time.
pub(crate) fn open_long(path: &Path) -> io::Result<File> {
    if path.as_os_str().len() < PATH_MAX {
        return File::open(path);
    }
    let mut held: Option<File> = None;
    let mut stretch = PathBuf::new();
    for part in path.components() {
        stretch.push(part);
        if stretch.as_os_str().len() >= STRETCH {
            held = Some(File::open(within(held.as_ref(), &stretch))?);
            stretch = PathBuf::new();
        }
    }
    match held {
        Some(dir) if stretch.as_os_str().is_empty() => Ok(dir),
        held => File::open(within(held.as_ref(), &stretch)),
    }
}

fn within(held: Option<&File>, stretch: &Path) -> PathBuf {
    held.map_or_else(|| stretch.to_path_buf(), |dir| held_path(dir).join(stretch))
}

#[cfg(test)]
#[path = "deeptree_tests.rs"]
mod tests;

#[cfg(test)]
#[test]
fn the_hand_declared_flags_match_the_target_abi() {
    assert_eq!(
        (W_OK, X_OK, EPERM, O_PATH),
        (libc::W_OK, libc::X_OK, libc::EPERM, libc::O_PATH)
    );
    assert_eq!(S_ISVTX, libc::S_ISVTX);
}
