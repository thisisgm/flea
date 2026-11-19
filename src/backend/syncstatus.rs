// A cloud sync engine's per-file state in user.sync.status, read per viewport row and never swept.
use flea::protocol::SyncMark;
use std::ffi::{c_char, c_void, CString};
use std::path::Path;

// The attribute a sync engine (rclone bisync wrappers such as Omarchy Storage Drives) writes on each file it keeps.
pub(crate) const ATTR_NAME: &str = "user.sync.status";
// The longest word SyncMark knows is 8 bytes; a longer value is no state Flea draws, so the read stops there.
const MAX_VALUE: usize = 16;

extern "C" {
    fn lgetxattr(path: *const c_char, name: *const c_char, value: *mut c_void, size: usize) -> isize;
}

// Sample input: a file tagged "synced" answers Some(Synced); untagged, a foreign word or a refusing filesystem answers None.
pub fn read(path: &Path) -> Option<SyncMark> {
    let cpath = CString::new(path.as_os_str().as_encoded_bytes()).ok()?;
    let name = CString::new(ATTR_NAME).expect("the attribute name holds no NUL");
    let mut buf = [0u8; MAX_VALUE];
    // One call: a value past the buffer fails ERANGE, which is no known word either.
    let got = unsafe { lgetxattr(cpath.as_ptr(), name.as_ptr(), buf.as_mut_ptr() as *mut c_void, buf.len()) };
    if got <= 0 {
        return None;
    }
    std::str::from_utf8(&buf[..got as usize]).ok().and_then(SyncMark::from_word)
}

// The row's optional `sy` field, the same conditional shape `tg` uses: empty for a row with no known state.
pub fn field(path: &Path) -> String {
    read(path).map_or_else(String::new, |mark| format!(r#","sy":"{}""#, mark.word()))
}

#[cfg(test)]
mod tests {
    use super::*;

    extern "C" {
        fn lsetxattr(path: *const c_char, name: *const c_char, value: *const c_void, size: usize, flags: i32) -> i32;
    }

    fn set(path: &Path, value: &[u8]) -> bool {
        let cpath = CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        let name = CString::new(ATTR_NAME).unwrap();
        unsafe { lsetxattr(cpath.as_ptr(), name.as_ptr(), value.as_ptr() as *const c_void, value.len(), 0) == 0 }
    }

    #[test]
    fn each_known_word_reads_as_its_mark_and_anything_else_reads_as_none() {
        let d = crate::backend::testdir::TestDir::new("syncstatus");
        let file = d.file("a.txt", "body");
        assert_eq!(read(&file), None, "no attribute");
        assert_eq!(field(&file), "");
        if !set(&file, b"synced") {
            // A filesystem without user xattrs answers None everywhere, which is what the reader promises.
            return;
        }
        for mark in SyncMark::ALL {
            assert!(set(&file, mark.word().as_bytes()));
            assert_eq!(read(&file), Some(mark));
        }
        assert!(set(&file, b"conflict"));
        assert_eq!(field(&file), r#","sy":"conflict""#);
        for foreign in [&b"Synced"[..], b"pending", b"", b"a-value-longer-than-the-buffer"] {
            assert!(set(&file, foreign));
            assert_eq!(read(&file), None, "{:?}", String::from_utf8_lossy(foreign));
        }
    }
}
