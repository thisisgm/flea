//! A menu's xattrs stay attached to an identity-checked Linux O_PATH descriptor.
use super::{menu_actions::Selected, tags};
use crate::{error::FleaError, oflags::O_NOFOLLOW};
use std::ffi::{c_char, c_int, c_void, CString};
use std::fs::{File, Metadata, OpenOptions};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::PathBuf;

const O_PATH: i32 = 0o10000000;
const ENODATA: i32 = 61;
const FILE_TYPE: u32 = 0o170000;

extern "C" {
    fn getxattr(path: *const c_char, name: *const c_char, value: *mut c_void, size: usize)
        -> isize;
    fn setxattr(
        path: *const c_char,
        name: *const c_char,
        value: *const c_void,
        size: usize,
        flags: c_int,
    ) -> c_int;
    fn removexattr(path: *const c_char, name: *const c_char) -> c_int;
}

pub(super) struct Pinned {
    file: File,
    path: PathBuf,
    descriptor: CString,
}

impl Pinned {
    pub(super) fn open(item: &Selected) -> Result<Self, String> {
        let before = item.current()?;
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(O_PATH | O_NOFOLLOW)
            .open(&item.path)
            .map_err(|error| crate::error::io_message(&error))?;
        let meta = file
            .metadata()
            .map_err(|error| crate::error::io_message(&error))?;
        if meta.dev() != before.dev()
            || meta.ino() != before.ino()
            || meta.mode() & FILE_TYPE != before.mode() & FILE_TYPE
            || item.retry_identity().is_some_and(|expected| {
                !expected.same_item(&super::undo::ItemIdentity::record(&meta))
            })
        {
            return Err("Selected item changed; reopen the menu.".into());
        }
        // O_PATH accepts mode 0000; following its proc descriptor operates on that held inode.
        let descriptor = CString::new(format!("/proc/self/fd/{}", file.as_raw_fd())).unwrap();
        Ok(Self {
            file,
            path: item.path.clone(),
            descriptor,
        })
    }

    pub(super) fn metadata(&self) -> std::io::Result<Metadata> {
        self.file.metadata()
    }
    pub(super) fn generation(&self) -> Option<u64> {
        super::filegen::of_fd(&self.file)
    }

    fn error(&self, component: &str) -> FleaError {
        crate::error::from_io(
            component,
            &self.path.to_string_lossy(),
            &std::io::Error::last_os_error(),
        )
    }

    pub(super) fn read_raw(&self) -> Result<Option<Vec<u8>>, FleaError> {
        let name = CString::new(tags::ATTR_NAME).unwrap();
        let size = unsafe {
            getxattr(
                self.descriptor.as_ptr(),
                name.as_ptr(),
                std::ptr::null_mut(),
                0,
            )
        };
        if size < 0 {
            return if std::io::Error::last_os_error().raw_os_error() == Some(ENODATA) {
                Ok(None)
            } else {
                Err(self.error("tags"))
            };
        }
        if size as usize > tags::MAX_VALUE {
            return Err(FleaError {
                where_: "tags".into(),
                path: self.path.to_string_lossy().into_owned(),
                msg: "the stored tags are too large to read".into(),
            });
        }
        let mut bytes = vec![0; size as usize];
        let got = unsafe {
            getxattr(
                self.descriptor.as_ptr(),
                name.as_ptr(),
                bytes.as_mut_ptr().cast(),
                bytes.len(),
            )
        };
        if got < 0 {
            return Err(self.error("tags"));
        }
        bytes.truncate(got as usize);
        Ok(Some(bytes))
    }

    pub(super) fn read_names(&self) -> Vec<String> {
        #[cfg(test)]
        super::tagreadprobe::observe(&self.path);
        if self.metadata().is_ok_and(|meta| meta.is_symlink()) {
            return Vec::new();
        }
        self.read_raw()
            .ok()
            .flatten()
            .map(|raw| tags::parse(&raw))
            .unwrap_or_default()
    }

    pub(super) fn restore(&self, raw: &Option<Vec<u8>>) -> Result<(), FleaError> {
        let name = CString::new(tags::ATTR_NAME).unwrap();
        let result = match raw {
            Some(bytes) => unsafe {
                setxattr(
                    self.descriptor.as_ptr(),
                    name.as_ptr(),
                    bytes.as_ptr().cast(),
                    bytes.len(),
                    0,
                )
            },
            None => unsafe { removexattr(self.descriptor.as_ptr(), name.as_ptr()) },
        };
        if result == 0
            || (raw.is_none() && std::io::Error::last_os_error().raw_os_error() == Some(ENODATA))
        {
            Ok(())
        } else {
            Err(self.error("settags"))
        }
    }

    pub(super) fn write_names(&self, names: &[String]) -> Result<(), FleaError> {
        self.restore(&(!names.is_empty()).then(|| tags::join(names)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;

    #[test]
    fn ey379_pinned_tag_read_keeps_the_captured_inode_after_path_replacement() {
        let d = TestDir::new("menu-tags-read-swap");
        let path = d.file("selected.txt", "original");
        tags::write_names(&path, &["Red".into()]).unwrap();
        let selected = Selected::inspect(&path.to_string_lossy()).unwrap();
        let pin = Pinned::open(&selected).unwrap();
        std::fs::rename(&path, d.join("original.txt")).unwrap();
        std::fs::write(&path, "replacement").unwrap();
        tags::write_names(&path, &["Blue".into()]).unwrap();
        assert_eq!(pin.read_names(), ["Red"]);
        assert_eq!(tags::read(&path).unwrap(), ["Blue"]);
    }
}
