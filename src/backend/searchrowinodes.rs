// File-inode coverage catches writes through any hard link while directory coverage guards path replacement.
use std::collections::HashSet;
use std::ffi::{c_char, c_int, c_void, CString};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::{Path, PathBuf};

const MASK: u32 = 0x0200_2fce;
extern "C" {
    fn inotify_init1(flags: c_int) -> c_int;
    fn inotify_add_watch(fd: c_int, path: *const c_char, mask: u32) -> c_int;
    fn read(fd: c_int, buf: *mut c_void, count: usize) -> isize;
}

#[derive(Default)]
pub(crate) struct Inodes {
    fd: Option<OwnedFd>,
    paths: HashSet<PathBuf>,
}

impl Inodes {
    fn arm(paths: &HashSet<PathBuf>) -> Option<Self> {
        if paths.len() > super::searchrowcache::ROW_CAP { return None; }
        let fd = unsafe { inotify_init1(0x0008_0800) };
        if fd < 0 { return None; }
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        for path in paths {
            let path = CString::new(path.as_os_str().as_encoded_bytes()).ok()?;
            if unsafe { inotify_add_watch(fd.as_raw_fd(), path.as_ptr(), MASK) } < 0 { return None; }
        }
        Some(Self { fd: Some(fd), paths: paths.clone() })
    }

    // Every event on this descriptor invalidates inode facts; any lost coverage also forces rearming.
    fn unchanged(&mut self) -> bool {
        let Some(fd) = self.fd.as_ref().map(AsRawFd::as_raw_fd) else { return false };
        let mut buffer = [0u8; 8192];
        for _ in 0..16 {
            let count = unsafe { read(fd, buffer.as_mut_ptr().cast(), buffer.len()) };
            if count < 0 {
                match std::io::Error::last_os_error().kind() {
                    std::io::ErrorKind::WouldBlock => return true,
                    std::io::ErrorKind::Interrupted => continue,
                    _ => break,
                }
            }
            break;
        }
        *self = Self::default();
        false
    }

    // Arm the replacement before draining the old set, keeping intersecting identities covered throughout handoff.
    pub(crate) fn cover(&mut self, paths: &HashSet<PathBuf>) -> bool {
        let clean = self.unchanged();
        if self.fd.is_some() && self.paths == *paths { return clean; }
        let Some(next) = Self::arm(paths) else { *self = Self::default(); return false };
        let bridged = self.unchanged();
        *self = next;
        clean && bridged
    }

    pub(crate) fn covered(&self, path: &Path) -> bool {
        self.fd.is_some() && self.paths.contains(path)
    }

    pub(crate) fn bytes(&self) -> usize {
        self.paths.iter().map(|path| path.as_os_str().len() + std::mem::size_of::<PathBuf>() + 64).sum()
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize { self.paths.len() }
}
