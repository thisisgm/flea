// Name/type enumeration with explicit read failures and a stat fallback only for DT_UNKNOWN.
use std::ffi::{c_char, c_int, CStr, CString, OsString};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::Path;

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
    fn opendir(path: *const c_char) -> *mut std::ffi::c_void;
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

fn read_with(dir: &Path, unknown: bool) -> std::io::Result<Directory> {
    let path = CString::new(dir.as_os_str().as_bytes())?;
    let handle = OpenDir(unsafe { opendir(path.as_ptr()) });
    if handle.0.is_null() {
        std::mem::forget(handle);
        return Err(std::io::Error::last_os_error());
    }
    let fd = unsafe { dirfd(handle.0) };
    let mut out = Directory { entries: Vec::new(), partial: None, #[cfg(test)] stats: 0 };
    loop {
        unsafe { *__errno_location() = 0 };
        let entry = unsafe { readdir(handle.0) };
        if entry.is_null() {
            if unsafe { *__errno_location() != 0 } { out.partial.get_or_insert("unreadable"); }
            return Ok(out);
        }
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
