//! Optional creation time, never inferred from inode change time.
use std::{ffi::CString, os::unix::ffi::OsStrExt, path::Path};

#[repr(C, align(8))]
struct Statx([u8; 256]);
extern "C" {
    fn statx(
        dirfd: i32,
        path: *const std::ffi::c_char,
        flags: i32,
        mask: u32,
        out: *mut Statx,
    ) -> i32;
}
const STATX_BTIME: u32 = 0x800;

pub fn get(path: &Path) -> Option<(i64, u32)> {
    let path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut out = Statx([0; 256]);
    // Linux statx ABI: no link following or automount, and no forced remote synchronization.
    if unsafe {
        statx(
            -100,
            path.as_ptr(),
            0x100 | 0x800 | 0x4000,
            STATX_BTIME,
            &mut out,
        )
    } != 0
    {
        return None;
    }
    decode(&out.0)
}
fn decode(bytes: &[u8; 256]) -> Option<(i64, u32)> {
    let mask = u32::from_ne_bytes(bytes[0..4].try_into().unwrap());
    if mask & STATX_BTIME == 0 {
        return None;
    }
    let seconds = i64::from_ne_bytes(bytes[80..88].try_into().unwrap());
    let nanos = u32::from_ne_bytes(bytes[88..92].try_into().unwrap());
    (nanos < 1_000_000_000).then_some((seconds, nanos))
}
pub fn json(path: &Path) -> String {
    get(path).map_or_else(|| "null".into(), |(seconds, _)| seconds.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_the_returned_btime_mask_authorizes_a_timestamp() {
        let mut bytes = [0; 256];
        bytes[80..88].copy_from_slice(&123i64.to_ne_bytes());
        assert_eq!(decode(&bytes), None);
        bytes[..4].copy_from_slice(&STATX_BTIME.to_ne_bytes());
        assert_eq!(decode(&bytes), Some((123, 0)));
        bytes[88..92].copy_from_slice(&1_000_000_000u32.to_ne_bytes());
        assert_eq!(decode(&bytes), None);
    }
    #[test]
    fn missing_paths_are_unavailable_and_links_are_not_followed() {
        let root = super::super::testdir::TestDir::new("birthtime");
        assert_eq!(get(&root.join("absent")), None);
        std::os::unix::fs::symlink("absent", root.join("link")).unwrap();
        let link = get(&root.join("link"));
        let directory = get(root.path());
        assert_eq!(link.is_some(), directory.is_some());
        if let Some((seconds, nanos)) = link {
            assert!(seconds > 0 && nanos < 1_000_000_000);
        }
    }
}
