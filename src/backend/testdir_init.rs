// Initialization owns a held directory until its marker is complete or rollback finishes.
use super::open_dir;
use std::fs::File;
use std::io;
use std::os::raw::{c_char, c_int};
use std::os::unix::fs::MetadataExt;
use std::os::unix::io::{AsRawFd, FromRawFd};
use std::path::Path;

const O_WRONLY: c_int = 1;
const O_CREAT: c_int = 0o100;
const O_EXCL: c_int = 0o200;
const O_CLOEXEC: c_int = 0o2000000;
const MARKER_NAME: &std::ffi::CStr = c".flea-test-sandbox";

extern "C" {
    fn openat(dirfd: c_int, path: *const c_char, flags: c_int, ...) -> c_int;
    fn unlinkat(dirfd: c_int, path: *const c_char, flags: c_int) -> c_int;
}

pub(super) struct UnmarkedDir<'a> {
    path: &'a Path,
    dir: Option<File>,
    identity: (u64, u64),
}

impl<'a> UnmarkedDir<'a> {
    pub(super) fn new(path: &'a Path) -> io::Result<Self> {
        if !path.is_absolute() || path.as_os_str().is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "a sandbox root must be absolute"));
        }
        let dir = open_dir(path)?;
        let meta = dir.metadata()?;
        Ok(Self { path, dir: Some(dir), identity: (meta.dev(), meta.ino()) })
    }

    pub(super) fn marker(&self) -> io::Result<File> {
        let dir = self.dir.as_ref().expect("initialization holds its directory");
        let flags = O_WRONLY | O_CREAT | O_EXCL | O_CLOEXEC | crate::oflags::O_NOFOLLOW;
        let fd = unsafe { openat(dir.as_raw_fd(), MARKER_NAME.as_ptr(), flags, 0o666_u32) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(unsafe { File::from_raw_fd(fd) })
    }

    pub(super) fn disarm(mut self) {
        self.dir = None;
    }

    fn is_current(&self) -> bool {
        self.path.symlink_metadata().is_ok_and(|meta| meta.is_dir() && (meta.dev(), meta.ino()) == self.identity)
    }
}

impl Drop for UnmarkedDir<'_> {
    fn drop(&mut self) {
        let Some(dir) = self.dir.as_ref() else { return };
        if !self.path.is_absolute() || self.path.as_os_str().is_empty() || !self.is_current() {
            return;
        }
        let removed = unsafe { unlinkat(dir.as_raw_fd(), MARKER_NAME.as_ptr(), 0) };
        if removed != 0 && io::Error::last_os_error().kind() != io::ErrorKind::NotFound {
            return;
        }
        if !self.is_current() {
            return;
        }
        // rmdir removes only an empty directory and POSIX has no descriptor-bound rmdir.
        if let Err(error) = std::fs::remove_dir(self.path) {
            eprintln!("test sandbox initialization left a directory behind: {}: {error}", self.path.display());
        }
    }
}
