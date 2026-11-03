use std::ffi::CString;
use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;

const O_TMPFILE: i32 = 0o20000000 | crate::oflags::O_DIRECTORY;
const O_CLOEXEC: i32 = 0o2000000;
const AT_REMOVEDIR: i32 = 0x200;

extern "C" {
    fn mkdirat(dir: i32, path: *const std::ffi::c_char, mode: u32) -> i32;
    fn openat(dir: i32, path: *const std::ffi::c_char, flags: i32, ...) -> i32;
    fn unlinkat(dir: i32, path: *const std::ffi::c_char, flags: i32) -> i32;
}

pub(super) fn open(root: &Path, delete_review: bool) -> io::Result<File> {
    let runtime = delete_review
        .then(|| crate::userfile::env_dir("XDG_RUNTIME_DIR"))
        .flatten();
    let local = delete_review.then_some(Path::new("/dev/shm"));
    open_with(root, runtime.as_deref(), local, temporary)
}

fn temporary(root: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .mode(0o600)
        .custom_flags(O_TMPFILE)
        .open(root)
}

fn unsupported(error: &io::Error) -> bool {
    matches!(error.raw_os_error(), Some(95 | 21 | 22))
}

fn anonymous(file: File) -> io::Result<File> {
    if file.metadata()?.nlink() != 0 {
        return Err(io::Error::from_raw_os_error(95));
    }
    Ok(file)
}

fn open_with(
    root: &Path,
    runtime: Option<&Path>,
    local: Option<&Path>,
    mut primary: impl FnMut(&Path) -> io::Result<File>,
) -> io::Result<File> {
    let mut last = match primary(root).and_then(anonymous) {
        Ok(file) => return Ok(file),
        Err(error) if unsupported(&error) => error,
        Err(error) => return Err(error),
    };
    // Copy manifests must stay on the filesystem their caller already qualified.
    if local.is_none() {
        return named_then_unlinked(root);
    }
    for candidate in [runtime.filter(|path| owned_runtime(path)), local]
        .into_iter()
        .flatten()
    {
        match primary(candidate).and_then(anonymous) {
            Ok(file) => return Ok(file),
            Err(error) if unsupported(&error) => match named_then_unlinked(candidate) {
                Ok(file) => return Ok(file),
                Err(error) => last = error,
            },
            Err(error) => last = error,
        }
    }
    Err(last)
}

fn owned_runtime(path: &Path) -> bool {
    path.is_absolute()
        && path.symlink_metadata().is_ok_and(|meta| {
            meta.is_dir()
                && meta.uid() == crate::backend::manifestdir::current_uid()
                && meta.mode() & 0o777 == 0o700
        })
}

struct DirectoryName {
    parent: File,
    name: CString,
}

impl DirectoryName {
    fn create(root: &Path) -> io::Result<Self> {
        let parent = OpenOptions::new()
            .read(true)
            .custom_flags(crate::oflags::O_DIRECTORY | O_CLOEXEC)
            .open(root)?;
        let meta = parent.metadata()?;
        let uid = crate::backend::manifestdir::current_uid();
        // A sticky root owned by us or root prevents another UID substituting the child.
        if (meta.uid() != uid && meta.uid() != 0)
            || (meta.mode() & 0o022 != 0 && meta.mode() & 0o1000 == 0)
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "review parent is not trusted",
            ));
        }
        for _ in 0..16 {
            let mut nonce = [0u8; 12];
            File::open("/dev/urandom")?.read_exact(&mut nonce)?;
            let suffix: String = nonce.iter().map(|byte| format!("{byte:02x}")).collect();
            let name = CString::new(format!(".flea-review.{suffix}")).map_err(io::Error::other)?;
            if unsafe { mkdirat(parent.as_raw_fd(), name.as_ptr(), 0o700) } == 0 {
                return Ok(Self { parent, name });
            }
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::AlreadyExists {
                return Err(error);
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "private review names collided",
        ))
    }
}

impl Drop for DirectoryName {
    fn drop(&mut self) {
        // Only an empty directory created by this attempt is eligible for removal.
        unsafe {
            unlinkat(self.parent.as_raw_fd(), self.name.as_ptr(), AT_REMOVEDIR);
        }
    }
}

struct PrivateDirectory {
    _entry: DirectoryName,
    directory: File,
    file_created: bool,
}

impl PrivateDirectory {
    fn create(root: &Path) -> io::Result<Self> {
        Self::create_checked(root, |directory| {
            let meta = directory.metadata()?;
            if meta.uid() != crate::backend::manifestdir::current_uid()
                || meta.mode() & 0o777 != 0o700
            {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "review directory is not private",
                ));
            }
            Ok(())
        })
    }

    fn create_checked(
        root: &Path,
        validate: impl FnOnce(&File) -> io::Result<()>,
    ) -> io::Result<Self> {
        let entry = DirectoryName::create(root)?;
        let flags = crate::oflags::O_DIRECTORY | crate::oflags::O_NOFOLLOW | O_CLOEXEC;
        let fd = unsafe { openat(entry.parent.as_raw_fd(), entry.name.as_ptr(), flags, 0u32) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let result = Self {
            _entry: entry,
            directory: unsafe { File::from_raw_fd(fd) },
            file_created: false,
        };
        validate(&result.directory)?;
        Ok(result)
    }
}

impl Drop for PrivateDirectory {
    fn drop(&mut self) {
        if self.file_created {
            unsafe {
                unlinkat(self.directory.as_raw_fd(), c"review".as_ptr(), 0);
            }
        }
    }
}

fn named_then_unlinked(root: &Path) -> io::Result<File> {
    create_review(PrivateDirectory::create(root)?)
}

fn create_review(mut private: PrivateDirectory) -> io::Result<File> {
    // O_RDWR | O_CREAT | O_EXCL, with atomic close-on-exec and no symlink following.
    let flags = 2 | 0o100 | 0o200 | O_CLOEXEC | crate::oflags::O_NOFOLLOW;
    let fd = unsafe {
        openat(
            private.directory.as_raw_fd(),
            c"review".as_ptr(),
            flags,
            0o600u32,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    private.file_created = true;
    let file = unsafe { File::from_raw_fd(fd) };
    if unsafe { unlinkat(private.directory.as_raw_fd(), c"review".as_ptr(), 0) } != 0 {
        return Err(io::Error::last_os_error());
    }
    private.file_created = false;
    anonymous(file)
}

#[cfg(test)]
mod tests;
