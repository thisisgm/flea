// Linux's atomic no-clobber rename, plus classification for the one mount that needs a safe caller-owned fallback.
use std::ffi::{c_char, CString, OsString};
use std::io;
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};

// Both paths passed here are absolute, so renameat2 never consults AT_FDCWD.
const AT_FDCWD: i32 = -100;
const RENAME_NOREPLACE: u32 = 1;
const EINVAL: i32 = 22;

extern "C" {
    fn renameat2(
        olddirfd: i32,
        oldpath: *const c_char,
        newdirfd: i32,
        newpath: *const c_char,
        flags: u32,
    ) -> i32;
}

// rclone rejects directory RENAME_NOREPLACE; callers that can safely copy and remove handle that case themselves.
pub fn rename_noreplace(from: &Path, to: &Path) -> io::Result<()> {
    let c_from = path_c(from)?;
    let c_to = path_c(to)?;
    let rc = unsafe {
        renameat2(
            AT_FDCWD,
            c_from.as_ptr(),
            AT_FDCWD,
            c_to.as_ptr(),
            RENAME_NOREPLACE,
        )
    };
    if rc == 0 {
        return Ok(());
    }
    Err(io::Error::last_os_error())
}

pub(crate) fn needs_copy_fallback(from: &Path, error: &io::Error) -> bool {
    std::fs::read_to_string("/proc/self/mountinfo")
        .ok()
        .map(|body| needs_copy_fallback_in(from, error, &body))
        .unwrap_or(false)
}

fn needs_copy_fallback_in(from: &Path, error: &io::Error, mountinfo: &str) -> bool {
    error.raw_os_error() == Some(EINVAL)
        && from
            .symlink_metadata()
            .map(|meta| meta.file_type().is_dir())
            .unwrap_or(false)
        && mount_type_in(from, mountinfo).as_deref() == Some("fuse.rclone")
}

fn path_c(path: &Path) -> io::Result<CString> {
    CString::new(path.as_os_str().as_encoded_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains an interior NUL"))
}

// Sample: `2 1 0:9 / /home/pi/My\040Drive rw - fuse.rclone remote: rw`; longest enclosing mount wins after octal unescaping.
fn mount_type_in(path: &Path, body: &str) -> Option<String> {
    let mut best: Option<(usize, String)> = None;
    for line in body.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let split = match fields.iter().position(|field| *field == "-") {
            Some(value) => value,
            None => continue,
        };
        if fields.len() <= split + 1 || fields.len() < 5 {
            continue;
        }
        let mount = PathBuf::from(OsString::from_vec(unescape(fields[4])));
        if !path.starts_with(&mount) {
            continue;
        }
        let depth = mount.components().count();
        if best.as_ref().map(|(old, _)| depth >= *old).unwrap_or(true) {
            best = Some((depth, fields[split + 1].to_string()));
        }
    }
    best.map(|(_, kind)| kind)
}

fn unescape(field: &str) -> Vec<u8> {
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\'
            && i + 3 < bytes.len()
            && bytes[i + 1..=i + 3]
                .iter()
                .all(|b| (b'0'..=b'7').contains(b))
        {
            out.push((bytes[i + 1] - b'0') * 64 + (bytes[i + 2] - b'0') * 8 + bytes[i + 3] - b'0');
            i += 4;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;

    const EEXIST: i32 = 17;

    #[test]
    fn deepest_mount_identifies_rclone_and_decodes_its_path() {
        let info = "1 0 8:1 / / rw - ext4 /dev/a rw\n\
                    2 1 0:9 / /home/pi/My\\040Drive rw - fuse.rclone remote: rw\n\
                    3 2 0:10 / /home/pi/My\\040Drive/nested rw - tmpfs tmpfs rw\n";
        assert_eq!(
            mount_type_in(Path::new("/home/pi/My Drive/file"), info).as_deref(),
            Some("fuse.rclone")
        );
        assert_eq!(
            mount_type_in(Path::new("/home/pi/My Drive/nested/file"), info).as_deref(),
            Some("tmpfs")
        );
        assert_eq!(
            mount_type_in(Path::new("/elsewhere"), info).as_deref(),
            Some("ext4")
        );
    }

    #[test]
    fn malformed_mountinfo_is_ignored() {
        assert_eq!(mount_type_in(Path::new("/home/pi"), "junk\n"), None);
    }

    #[test]
    fn copy_fallback_scope_is_only_an_einval_directory_on_rclone() {
        let d = TestDir::new("rclonerenamescope");
        let file = d.file("file", "body");
        let directory = d.dir("directory");
        let rclone = format!(
            "1 0 0:1 / {} rw - fuse.rclone remote: rw\n",
            d.path().display()
        );
        let ext4 = format!("1 0 8:1 / {} rw - ext4 /dev/a rw\n", d.path().display());
        let invalid = io::Error::from_raw_os_error(EINVAL);
        let exists = io::Error::from_raw_os_error(EEXIST);
        assert!(needs_copy_fallback_in(&directory, &invalid, &rclone));
        assert!(!needs_copy_fallback_in(&file, &invalid, &rclone));
        assert!(!needs_copy_fallback_in(&directory, &invalid, &ext4));
        assert!(!needs_copy_fallback_in(&directory, &exists, &rclone));
        assert_eq!(std::fs::read_to_string(file).unwrap(), "body");
    }
}
