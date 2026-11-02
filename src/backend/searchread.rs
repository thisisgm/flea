// Name/type enumeration with explicit read failures and a stat fallback only for DT_UNKNOWN.
use std::ffi::{c_char, c_int, CStr, CString, OsString};
use std::collections::HashSet;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use crate::oflags::{O_DIRECTORY, O_NOFOLLOW};

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

const DENTS_BYTES: usize = 32768;
// A linux_dirent64 record: d_ino 1234 at 0, d_off at 8, d_reclen 32 at 16, d_type 8 (DT_REG) at 18, "a.txt\0" at 19, padded to 8 bytes.
const RECLEN_AT: usize = 16;
const TYPE_AT: usize = 18;
const NAME_AT: usize = 19;
const ENOENT: i32 = 2;

extern "C" {
    fn open(path: *const c_char, flags: c_int, ...) -> c_int;
    fn close(fd: c_int) -> c_int;
    fn getdents64(fd: c_int, buf: *mut std::ffi::c_void, count: usize) -> isize;
    fn fstatat(fd: c_int, path: *const c_char, buf: *mut std::ffi::c_void, flags: c_int) -> c_int;
}

struct OpenFd(c_int);
impl Drop for OpenFd {
    fn drop(&mut self) {
        unsafe { close(self.0) };
    }
}

fn directory_type(dtype: u8, fallback: impl FnOnce() -> std::io::Result<u32>) -> std::io::Result<bool> {
    if dtype != 0 {
        return Ok(dtype == 4);
    }
    fallback().map(|mode| mode & 0o170000 == 0o040000)
}

#[cfg(test)]
pub(crate) fn read(dir: &Path) -> std::io::Result<Directory> {
    read_with(dir, false)
}

// An explicitly chosen scope may be a symlink; recursively discovered directories may not.
#[cfg(test)]
pub(crate) fn read_root(dir: &Path) -> std::io::Result<Directory> {
    read_observing(dir, false, |_| {}, true, || false)
}

#[cfg(test)]
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

// Raw getdents64 into a stack buffer: fdopendir would add an fstat, two fcntl calls and a 32 KiB heap buffer per directory.
fn read_observing(dir: &Path, unknown: bool, mut progress: impl FnMut(usize), follow: bool, stop: impl Fn() -> bool) -> std::io::Result<Directory> {
    let path = CString::new(dir.as_os_str().as_bytes())?;
    let flags = 0x80000 | O_DIRECTORY | if follow { 0 } else { O_NOFOLLOW };
    let raw = unsafe { open(path.as_ptr(), flags) };
    if raw < 0 { return Err(std::io::Error::last_os_error()); }
    let handle = OpenFd(raw);
    let fd = handle.0;
    let mut out = Directory { entries: Vec::new(), partial: None, #[cfg(test)] stats: 0 };
    let mut buf = std::mem::MaybeUninit::<[u64; DENTS_BYTES / 8]>::uninit();
    loop {
        if stop() { out.partial = Some("cancelled"); return Ok(out); }
        let read = unsafe { getdents64(fd, buf.as_mut_ptr().cast(), DENTS_BYTES) };
        if read <= 0 {
            // As glibc's readdir does, ENOENT from a directory removed while open reads as its end.
            if read < 0 && std::io::Error::last_os_error().raw_os_error() != Some(ENOENT) { out.partial.get_or_insert("unreadable"); }
            return Ok(out);
        }
        let base = buf.as_ptr().cast::<u8>();
        let mut at = 0;
        while at < read as usize {
            let entry = unsafe { base.add(at) };
            let reclen = unsafe { std::ptr::read_unaligned(entry.add(RECLEN_AT).cast::<u16>()) } as usize;
            if reclen == 0 { out.partial.get_or_insert("unreadable"); return Ok(out); }
            at += reclen;
            // glibc's readdir skips a record whose inode is 0, a deleted entry.
            if unsafe { std::ptr::read_unaligned(entry.cast::<u64>()) } == 0 { continue; }
            if stop() { out.partial = Some("cancelled"); return Ok(out); }
            let name = unsafe { CStr::from_ptr(entry.add(NAME_AT).cast::<c_char>()) };
            let bytes = name.to_bytes();
            if bytes == b"." || bytes == b".." { continue; }
            let dtype = if unknown { 0 } else { unsafe { *entry.add(TYPE_AT) } };
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;

    #[test]
    fn arm_oflags_search_refuses_symlinks_unless_following_is_requested() {
        let dir = TestDir::new("arm-oflags-search");
        let target = dir.dir("target");
        std::fs::write(target.join("file"), "body").unwrap();
        let link = dir.join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(
            read(&link).is_err(),
            "a directory symlink must be refused at open"
        );
        let followed = read_root(&link).unwrap();
        assert!(followed.partial.is_none());
        assert_eq!(followed.entries, vec![(OsString::from("file"), false)]);
        let plain = dir.file("plain", "body");
        assert!(
            read(&plain).is_err(),
            "a regular file must be refused as a directory"
        );
    }

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
    fn search_read_spans_several_getdents_buffers() {
        let d = TestDir::new("searchmany");
        // 1200 names of 60 bytes take about 96 KiB of dirent records, three full reads.
        for i in 0..1200 {
            let name = format!("{i:04}-{}", "n".repeat(55));
            if i % 7 == 0 { d.dir(&name); } else { d.file(&name, ""); }
        }
        let read = read(d.path()).unwrap();
        assert!(read.partial.is_none());
        assert_eq!(read.stats, 0);
        let mut got = read.entries;
        got.sort();
        let mut want: Vec<(OsString, bool)> = std::fs::read_dir(d.path()).unwrap()
            .map(|entry| { let entry = entry.unwrap(); (entry.file_name(), entry.file_type().unwrap().is_dir()) })
            .collect();
        want.sort();
        // The sandbox root also holds its own marker file.
        assert_eq!(got.len(), 1201);
        assert_eq!(got, want);
        let forced = read_with(d.path(), true).unwrap();
        assert_eq!(forced.stats, want.len());
        let mut unknown = forced.entries;
        unknown.sort();
        assert_eq!(unknown, want);
    }

    #[test]
    fn search_unknown_type_failure_is_not_a_complete_entry() {
        assert!(directory_type(0, || Err(std::io::Error::from_raw_os_error(13))).is_err());
    }
}
