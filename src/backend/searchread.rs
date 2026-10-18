// Name/type enumeration with explicit read failures and a stat fallback only for DT_UNKNOWN.
use std::ffi::{c_char, c_int, CStr, CString, OsString};
use std::collections::HashSet;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

// A cancelled caller may leave one syscall behind; another session must not start a second reader on that mount.
fn active_mounts() -> &'static Mutex<HashSet<PathBuf>> {
    static ACTIVE: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
    ACTIVE.get_or_init(Default::default)
}

struct MountRead(PathBuf);

impl MountRead {
    fn take(dir: &Path, body: &str) -> Result<Option<Self>, crate::error::FleaError> {
        if !super::iomount::is_remote(dir, body) { return Ok(None); }
        let mount = super::iomount::mount_key(dir, body);
        if !active_mounts().lock().unwrap_or_else(|poisoned| poisoned.into_inner()).insert(mount.clone()) {
            return Err(crate::error::FleaError {
                where_: "search".to_string(), path: dir.to_string_lossy().into_owned(),
                msg: format!("{} is not responding.", mount.display()),
            });
        }
        Ok(Some(Self(mount)))
    }
}

impl Drop for MountRead {
    fn drop(&mut self) {
        active_mounts().lock().unwrap_or_else(|poisoned| poisoned.into_inner()).remove(&self.0);
    }
}

pub(crate) struct Directory {
    pub entries: Vec<(OsString, bool)>,
    pub partial: Option<&'static str>,
    #[cfg(test)]
    pub stats: usize,
}

#[repr(C)]
struct Dirent {
    ino: u64,
    off: i64,
    reclen: u16,
    dtype: u8,
    name: [c_char; 256],
}

extern "C" {
    fn open(path: *const c_char, flags: c_int, ...) -> c_int;
    fn close(fd: c_int) -> c_int;
    fn fdopendir(fd: c_int) -> *mut std::ffi::c_void;
    fn readdir(dir: *mut std::ffi::c_void) -> *mut Dirent;
    fn closedir(dir: *mut std::ffi::c_void) -> c_int;
    fn dirfd(dir: *mut std::ffi::c_void) -> c_int;
    fn fstatat(fd: c_int, path: *const c_char, buf: *mut std::ffi::c_void, flags: c_int) -> c_int;
    fn __errno_location() -> *mut c_int;
}

struct OpenDir(*mut std::ffi::c_void);
impl Drop for OpenDir {
    fn drop(&mut self) {
        unsafe { closedir(self.0) };
    }
}

fn directory_type(dtype: u8, fallback: impl FnOnce() -> std::io::Result<u32>) -> std::io::Result<bool> {
    if dtype != 0 {
        return Ok(dtype == 4);
    }
    fallback().map(|mode| mode & 0o170000 == 0o040000)
}

pub(crate) fn read(dir: &Path) -> std::io::Result<Directory> {
    read_with(dir, false)
}

// An explicitly chosen scope may be a symlink; recursively discovered directories may not.
pub(crate) fn read_root(dir: &Path) -> std::io::Result<Directory> {
    read_observing(dir, false, |_| {}, true, || false)
}

fn read_with(dir: &Path, unknown: bool) -> std::io::Result<Directory> {
    read_observing(dir, unknown, |_| {}, false, || false)
}

pub(crate) fn read_interruptible(dir: &Path, follow: bool, stop: impl Fn() -> bool, progress: impl FnMut(usize)) -> std::io::Result<Directory> {
    if stop() { return Ok(Directory { entries: Vec::new(), partial: Some("cancelled"), #[cfg(test)] stats: 0 }); }
    read_observing(dir, false, progress, follow, stop)
}

// An abandoned remote reader stops before its next entry when its blocked syscall returns.
pub(crate) fn bounded(dir: &Path, body: &str, follow: bool) -> Result<std::io::Result<Directory>, crate::error::FleaError> {
    bounded_observed(dir, body, follow, || false, |_| {})
}

pub(crate) fn bounded_observed(
    dir: &Path, body: &str, follow: bool,
    stop: impl Fn() -> bool + Send + 'static,
    progress: impl FnMut(usize) + Send + 'static,
) -> Result<std::io::Result<Directory>, crate::error::FleaError> {
    use std::sync::{atomic::{AtomicBool, Ordering}, Arc};
    let lease = MountRead::take(dir, body)?;
    let abandoned = Arc::new(AtomicBool::new(false));
    let worker = Arc::clone(&abandoned);
    let owned = dir.to_path_buf();
    let result = super::iomount::call(dir, body, "search", move || {
        let _lease = lease;
        read_interruptible(&owned, follow, || stop() || worker.load(Ordering::Acquire), progress)
    });
    abandoned.store(true, Ordering::Release);
    result
}

fn read_observing(dir: &Path, unknown: bool, mut progress: impl FnMut(usize), follow: bool, stop: impl Fn() -> bool) -> std::io::Result<Directory> {
    let path = CString::new(dir.as_os_str().as_bytes())?;
    let flags = 0x80000 | 0x10000 | if follow { 0 } else { 0x20000 };
    let raw = unsafe { open(path.as_ptr(), flags) };
    if raw < 0 { return Err(std::io::Error::last_os_error()); }
    let handle = OpenDir(unsafe { fdopendir(raw) });
    if handle.0.is_null() {
        let error = std::io::Error::last_os_error();
        unsafe { close(raw); }
        std::mem::forget(handle);
        return Err(error);
    }
    let fd = unsafe { dirfd(handle.0) };
    let mut out = Directory { entries: Vec::new(), partial: None, #[cfg(test)] stats: 0 };
    loop {
        if stop() { out.partial = Some("cancelled"); return Ok(out); }
        unsafe { *__errno_location() = 0 };
        let entry = unsafe { readdir(handle.0) };
        if entry.is_null() {
            if unsafe { *__errno_location() != 0 } { out.partial.get_or_insert("unreadable"); }
            return Ok(out);
        }
        if stop() { out.partial = Some("cancelled"); return Ok(out); }
        let name = unsafe { CStr::from_ptr((*entry).name.as_ptr()) };
        let bytes = name.to_bytes();
        if bytes == b"." || bytes == b".." { continue; }
        let dtype = if unknown { 0 } else { unsafe { (*entry).dtype } };
        let kind = directory_type(dtype, || {
            #[cfg(test)]
            { out.stats += 1; }
            let mut buf = [0u64; 64];
            if unsafe { fstatat(fd, name.as_ptr(), buf.as_mut_ptr().cast(), 0x100) } != 0 {
                return Err(std::io::Error::last_os_error());
            }
            #[cfg(target_arch = "x86_64")]
            let off = 24;
            #[cfg(not(target_arch = "x86_64"))]
            let off = 16;
            let mode = unsafe { std::ptr::read_unaligned(buf.as_ptr().cast::<u8>().add(off).cast::<u32>()) };
            Ok(mode)
        });
        match kind {
            Ok(is_dir) => {
                let mut raw = Vec::new();
                if raw.try_reserve_exact(bytes.len()).is_err() || out.entries.try_reserve(1).is_err() {
                    out.partial = Some("allocation");
                    return Ok(out);
                }
                raw.extend_from_slice(bytes);
                out.entries.push((OsString::from_vec(raw), is_dir));
                progress(out.entries.len());
            }
            Err(_) => { out.partial.get_or_insert("unreadable"); }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;

    #[test]
    fn search_typed_entries_never_call_the_metadata_fallback() {
        for dtype in [1, 2, 4, 6, 8, 10, 12] {
            assert_eq!(directory_type(dtype, || panic!("typed entries must not stat")).unwrap(), dtype == 4);
        }
        let d = TestDir::new("searchtyped");
        d.file("file", "");
        d.dir("dir");
        std::os::unix::fs::symlink(d.join("dir"), d.join("link")).unwrap();
        let read = read(d.path()).unwrap();
        assert_eq!(read.stats, 0);
        assert!(read.partial.is_none());
        let forced = read_with(d.path(), true).unwrap();
        assert_eq!(forced.stats, forced.entries.len());
        let mut normal = read.entries;
        let mut unknown = forced.entries;
        normal.sort();
        unknown.sort();
        assert_eq!(normal, unknown);
        assert!(normal.contains(&(OsString::from("link"), false)));
    }

    #[test]
    fn search_unknown_type_failure_is_not_a_complete_entry() {
        assert!(directory_type(0, || Err(std::io::Error::from_raw_os_error(13))).is_err());
    }
}
