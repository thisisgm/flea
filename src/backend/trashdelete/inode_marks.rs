use super::*;
use std::os::fd::FromRawFd;

extern "C" {
    fn inotify_init1(flags: i32) -> i32;
    fn inotify_add_watch(fd: i32, path: *const std::ffi::c_char, mask: u32) -> i32;
}

pub(in crate::backend::trashdelete) struct InodeMarks(File);
impl InodeMarks {
    pub(in crate::backend::trashdelete) fn new() -> Result<Self, Refusal> {
        // SAFETY: no pointers are passed; the successful descriptor is owned below.
        let raw = unsafe { inotify_init1(super::super::super::watch::IN_CLOEXEC) };
        if raw < 0 {
            return Err(watch_failed(std::io::Error::last_os_error()));
        }
        // SAFETY: raw is a fresh descriptor transferred exclusively to File.
        Ok(Self(unsafe { File::from_raw_fd(raw) }))
    }

    pub(in crate::backend::trashdelete) fn descriptor(&self, fd: i32) -> Result<i32, Refusal> {
        self.mark(
            Path::new(&format!("/proc/self/fd/{fd}")),
            super::super::super::watch::IN_ATTRIB,
        )
    }

    pub(in crate::backend::trashdelete) fn path(&self, path: &Path) -> Result<i32, Refusal> {
        self.mark(
            path,
            super::super::super::watch::IN_ATTRIB | super::super::super::watch::IN_DONT_FOLLOW,
        )
    }

    fn mark(&self, path: &Path, flags: u32) -> Result<i32, Refusal> {
        let name = CString::new(path.as_os_str().as_bytes()).map_err(|_| changed())?;
        // SAFETY: self owns the descriptor and name is a live NUL-terminated path.
        let wd = unsafe { inotify_add_watch(self.0.as_raw_fd(), name.as_ptr(), flags) };
        if wd < 0 {
            Err(watch_failed(std::io::Error::last_os_error()))
        } else {
            Ok(wd)
        }
    }
}
