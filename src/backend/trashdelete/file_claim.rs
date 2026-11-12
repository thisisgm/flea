// A held dentry proves which file the rename moved, even when FUSE renumbers its inode.
use super::*;

const O_PATH: i32 = 0o10000000;
const MAX_REVIEW_HANDLES: usize = 256;
static HELD_REVIEW_HANDLES: AtomicUsize = AtomicUsize::new(0);

struct ReviewHold;
impl ReviewHold {
    // try_update needs Rust 1.95, newer than the workspace rust-version 1.77, so fetch_update stays.
    #[allow(deprecated)]
    fn take() -> Option<Self> {
        let limit = MAX_REVIEW_HANDLES;
        #[cfg(test)]
        let limit = hold_tests::limit(limit);
        HELD_REVIEW_HANDLES
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |held| {
                (held < limit).then_some(held + 1)
            })
            .ok()
            .map(|_| Self)
    }
}
impl Drop for ReviewHold {
    fn drop(&mut self) {
        HELD_REVIEW_HANDLES.fetch_sub(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
#[path = "cache_tests.rs"]
mod cache_tests;
#[cfg(test)]
#[path = "hold_tests.rs"]
mod hold_tests;

pub(super) struct FileClaim {
    held: File,
    expected: Identity,
    original: Option<PathBuf>,
    _review_hold: Option<ReviewHold>,
}

fn changed() -> Refusal {
    Refusal::new(
        Outcome::ItemChanged,
        "Item changed after confirmation; no deletion attempted.",
    )
}

// A vanished item is a change; any other failure to hold it is reported as the system said it.
fn hold_failed(error: std::io::Error) -> Refusal {
    if error.kind() == ErrorKind::NotFound {
        return changed();
    }
    Refusal::os(
        Outcome::Prepare(Op::Delete),
        "Could not hold the item for deletion",
        &error,
    )
}

// A staged item that vanished is a change; a watch limit or permission error is reported as such.
fn watch_failed(error: std::io::Error) -> Refusal {
    if error.kind() == ErrorKind::NotFound {
        return changed();
    }
    Refusal::os(
        Outcome::ChildLeft,
        "Could not compare the staged item with the held one",
        &error,
    )
}

fn same_metadata(expected: &Identity, current: &Identity) -> bool {
    expected.dev == current.dev
        && expected.size == current.size
        && expected.mtime == current.mtime
        && expected.nanos == current.nanos
        && expected.mode == current.mode
}

impl FileClaim {
    pub(super) fn for_review(path: &Path, expected: &Identity) -> Result<Option<Self>, Refusal> {
        if expected.mode & 0o170000 == 0o040000 {
            return Ok(None);
        }
        let parent_path = path.parent().ok_or_else(changed)?;
        let magic = crate::backend::fsinfo::magic_of(parent_path);
        let retain = magic.is_none() || magic == Some(crate::backend::copyread::FUSE_MAGIC);
        #[cfg(test)]
        let retain = retain || super::claim_tests::simulated_fuse() || hold_tests::simulated_fuse();
        if !retain {
            return Ok(None);
        }
        let Some(hold) = ReviewHold::take() else {
            return Ok(None);
        };
        let parent = open_dir(parent_path)?;
        let name = path.file_name().ok_or_else(changed)?;
        let mut claim = Self::open(&parent, name, expected)?;
        claim._review_hold = Some(hold);
        claim.original = Some(location(&parent)?.join(name));
        if !claim.unchanged() {
            return Err(changed());
        }
        Ok(Some(claim))
    }

    pub(super) fn open(parent: &File, name: &OsStr, expected: &Identity) -> Result<Self, Refusal> {
        #[cfg(test)]
        if let Some(error) = hold_tests::open_error() {
            return Err(hold_failed(error));
        }
        let held = OpenOptions::new()
            .read(true)
            .custom_flags(O_PATH | O_NOFOLLOW)
            .open(fd_path(parent, name))
            .map_err(hold_failed)?;
        if Identity::of(&held.metadata().map_err(hold_failed)?) != *expected {
            return Err(changed());
        }
        Ok(Self {
            held,
            expected: expected.clone(),
            original: None,
            _review_hold: None,
        })
    }

    pub(super) fn duplicate(&self) -> Result<Self, Refusal> {
        Ok(Self {
            held: self.held.try_clone().map_err(hold_failed)?,
            expected: self.expected.clone(),
            original: self.original.clone(),
            _review_hold: None,
        })
    }

    pub(super) fn unchanged(&self) -> bool {
        self.original.as_ref().map_or(true, |original| {
            location(&self.held).ok().as_ref() == Some(original)
        })
    }

    pub(super) fn remove(self, quarantine: &File) -> Result<(), Refusal> {
        let path = fd_path(quarantine, OsStr::new("payload"));
        let landed = staged_identity(&path).map_err(|_| changed())?;
        if !same_metadata(&self.expected, &landed) {
            return Err(changed());
        }
        if (self.original.is_some() || landed != self.expected) && !self.moved_into(quarantine)? {
            return Err(changed());
        }
        let needs_inode_proof = if landed != self.expected {
            let held_metadata = Identity::of(&self.held.metadata().map_err(|_| changed())?);
            if !same_metadata(&self.expected, &held_metadata) {
                return Err(changed());
            }
            !renumbers(quarantine)?
        } else {
            false
        };
        let current = staged_identity(&path).map_err(|_| changed())?;
        if (needs_inode_proof || current != landed)
            && (!same_metadata(&self.expected, &current) || !self.same_kernel_inode(&path)?)
        {
            return Err(changed());
        }
        // Only this operation populates its private quarantine; no second staging rename is needed.
        std::fs::remove_file(&path).refuse(Outcome::ChildLeft, "Trash child could not be removed")
    }

    fn moved_into(&self, quarantine: &File) -> Result<bool, Refusal> {
        let held = location(&self.held)?;
        let folder = location(quarantine)?;
        let expected = folder.join("payload");
        if held == expected {
            return Ok(true);
        }
        // FUSE invalidates the renamed dentry; proc still identifies its last location.
        let mut invalidated = expected.into_os_string();
        invalidated.push(" (deleted)");
        Ok(held.as_os_str() == invalidated)
    }

    // Watch descriptors compare kernel inode objects, even if FUSE changes their reported numbers.
    pub(super) fn same_kernel_inode(&self, path: &Path) -> Result<bool, Refusal> {
        use std::os::fd::FromRawFd;
        extern "C" {
            fn inotify_init1(flags: i32) -> i32;
            fn inotify_add_watch(fd: i32, path: *const std::ffi::c_char, mask: u32) -> i32;
        }
        // SAFETY: no pointers are passed; a successful descriptor is owned below.
        let raw = unsafe { inotify_init1(super::super::watch::IN_CLOEXEC) };
        if raw < 0 {
            return Err(watch_failed(std::io::Error::last_os_error()));
        }
        // SAFETY: raw is a fresh descriptor transferred exclusively to this File.
        let watches = unsafe { File::from_raw_fd(raw) };
        let held = CString::new(format!("/proc/self/fd/{}", self.held.as_raw_fd()))
            .map_err(|_| changed())?;
        let staged = CString::new(path.as_os_str().as_bytes()).map_err(|_| changed())?;
        let mark = |name: &CString, flags| {
            // SAFETY: watches owns the descriptor and name is a live NUL-terminated path.
            let wd = unsafe { inotify_add_watch(watches.as_raw_fd(), name.as_ptr(), flags) };
            if wd < 0 {
                Err(watch_failed(std::io::Error::last_os_error()))
            } else {
                Ok(wd)
            }
        };
        let selected = mark(&held, super::super::watch::IN_ATTRIB)?;
        Ok(mark(
            &staged,
            super::super::watch::IN_ATTRIB | super::super::watch::IN_DONT_FOLLOW,
        )? == selected)
    }
}

fn staged_identity(path: &Path) -> std::io::Result<Identity> {
    let result = identity(path);
    #[cfg(test)]
    let result = cache_tests::read(path, result);
    result
}

fn location(file: &File) -> Result<PathBuf, Refusal> {
    std::fs::read_link(format!("/proc/self/fd/{}", file.as_raw_fd())).map_err(|_| changed())
}

// An inode mismatch on a stable filesystem is a replacement, never a compatibility exception.
pub(super) fn renumbers(quarantine: &File) -> Result<bool, Refusal> {
    let before = OsStr::new("identity-before");
    let after = OsStr::new("identity-after");
    let probe = |error: std::io::Error| {
        Refusal::os(
            Outcome::ChildLeft,
            "Could not test how this mount numbers renamed files",
            &error,
        )
    };
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(fd_path(quarantine, before))
        .map_err(probe)?;
    let original = identity(&fd_path(quarantine, before));
    let moved = original.is_ok() && rename(quarantine, before, quarantine, after).is_ok();
    let name = if moved { after } else { before };
    let current = identity(&fd_path(quarantine, name));
    let removed = std::fs::remove_file(fd_path(quarantine, name));
    drop(file);
    removed.map_err(probe)?;
    let (original, current) = (original.map_err(probe)?, current.map_err(probe)?);
    Ok(moved && original.dev == current.dev && original.ino != current.ino)
}
