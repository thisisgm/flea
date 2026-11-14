//! Non-triggering metadata and pinned directory opens for folder-size walks.
#[cfg(test)]
use crate::backend::copyfile::held_path;
use std::ffi::{c_char, CString, OsStr};
use std::fs::{File, OpenOptions};
use std::os::fd::AsRawFd;
use std::os::unix::{ffi::OsStrExt, fs::OpenOptionsExt};
use std::path::Path;

const AT_FDCWD: i32 = -100;
const AT_SYMLINK_NOFOLLOW: i32 = 0x100;
const AT_NO_AUTOMOUNT: i32 = 0x800;
const AT_EMPTY_PATH: i32 = 0x1000;
const AT_STATX_DONT_SYNC: i32 = 0x4000;
const STATX_BASIC_STATS: u32 = 0x7ff;
const STATX_MNT_ID: u32 = 0x1000;
const STATX_ATTR_AUTOMOUNT: u64 = 0x1000;
const O_PATH: i32 = 0o10000000;
// Type, link count, inode and size, without which a walk cannot count an entry.
const STATX_REQUIRED: u32 = STATX_TYPE | STATX_NLINK | STATX_INO | STATX_SIZE;
const STATX_TYPE: u32 = 0x1;
const STATX_NLINK: u32 = 0x4;
const STATX_INO: u32 = 0x100;
const STATX_SIZE: u32 = 0x200;
const S_IFMT: u16 = 0o170000;
const S_IFDIR: u16 = 0o040000;
const S_IFREG: u16 = 0o100000;
// Byte offsets into struct statx from include/uapi/linux/stat.h, identical on x86_64 and arm64.
const STX_MASK: usize = 0;
const STX_ATTRIBUTES: usize = 8;
const STX_NLINK: usize = 16;
const STX_MODE: usize = 28;
const STX_INO: usize = 32;
const STX_SIZE: usize = 40;
const STX_ATTRIBUTES_MASK: usize = 56;
const STX_DEV_MAJOR: usize = 136;
const STX_DEV_MINOR: usize = 140;
const STX_MNT_ID: usize = 144;

#[repr(C, align(8))]
struct Statx([u8; 256]);
extern "C" {
    fn statx(fd: i32, path: *const c_char, flags: i32, mask: u32, out: *mut Statx) -> i32;
}

pub(super) type Identity = (u64, u64, Option<u64>);

#[derive(Clone, Copy, Debug)]
pub(crate) struct Entry {
    pub dev: u64,
    pub ino: u64,
    pub size: u64,
    pub nlink: u64,
    pub mount: Option<u64>,
    pub automount: bool,
    kind: u16,
}

impl Entry {
    pub fn identity(&self) -> Identity {
        (self.dev, self.ino, self.mount)
    }

    pub fn is_dir(&self) -> bool {
        self.kind == S_IFDIR
    }

    pub fn is_file(&self) -> bool {
        self.kind == S_IFREG
    }
}

pub(crate) fn at(path: &Path) -> Option<Entry> {
    probe(path, false)
}

pub(super) fn probe(path: &Path, sync: bool) -> Option<Entry> {
    let flags = if sync { 0 } else { AT_STATX_DONT_SYNC };
    let entry = query(
        AT_FDCWD,
        &CString::new(path.as_os_str().as_bytes()).ok()?,
        flags,
    )?;
    #[cfg(test)]
    let entry = fake::apply(path, entry);
    Some(entry)
}

pub(super) fn of(dir: &File) -> Option<Entry> {
    let entry = query(dir.as_raw_fd(), c"", AT_EMPTY_PATH | AT_STATX_DONT_SYNC)?;
    #[cfg(test)]
    let entry = fake::apply_held(dir, entry);
    Some(entry)
}

pub(super) fn child(dir: &File, name: &OsStr, sync: bool) -> Option<Entry> {
    let flags = if sync { 0 } else { AT_STATX_DONT_SYNC };
    let entry = query(dir.as_raw_fd(), &CString::new(name.as_bytes()).ok()?, flags)?;
    #[cfg(test)]
    let entry = fake::apply_child(dir, name, entry);
    Some(entry)
}

fn query(fd: i32, path: &std::ffi::CStr, extra: i32) -> Option<Entry> {
    query_with(fd, path, extra, |fd, path, flags, mask, out| {
        // The path and output stay live for the call and Statx has the Linux layout on both supported architectures.
        unsafe { statx(fd, path, flags, mask, out) }
    })
}

fn query_with(
    fd: i32,
    path: &std::ffi::CStr,
    extra: i32,
    syscall: impl FnOnce(i32, *const c_char, i32, u32, *mut Statx) -> i32,
) -> Option<Entry> {
    let mut out = Statx([0; 256]);
    let flags = AT_SYMLINK_NOFOLLOW | AT_NO_AUTOMOUNT | extra;
    if syscall(
        fd,
        path.as_ptr(),
        flags,
        STATX_BASIC_STATS | STATX_MNT_ID,
        &mut out,
    ) != 0
    {
        return None;
    }
    decode(&out.0)
}

// Sample input: the 256-byte buffer statx(2) filled, read at the STX_* offsets; glibc's makedev packs the device.
fn decode(bytes: &[u8; 256]) -> Option<Entry> {
    let u32_at = |at: usize| u32::from_ne_bytes(std::array::from_fn(|i| bytes[at + i]));
    let u64_at = |at: usize| u64::from_ne_bytes(std::array::from_fn(|i| bytes[at + i]));
    let mask = u32_at(STX_MASK);
    // An unavailable required field leaves a partial total rather than a guessed one.
    if mask & STATX_REQUIRED != STATX_REQUIRED {
        return None;
    }
    let major = u32_at(STX_DEV_MAJOR) as u64;
    let minor = u32_at(STX_DEV_MINOR) as u64;
    Some(Entry {
        dev: ((major & 0xfff) << 8)
            | ((major & 0xffff_f000) << 32)
            | (minor & 0xff)
            | ((minor & 0xffff_ff00) << 12),
        ino: u64_at(STX_INO),
        size: u64_at(STX_SIZE),
        nlink: u32_at(STX_NLINK) as u64,
        mount: (mask & STATX_MNT_ID != 0).then(|| u64_at(STX_MNT_ID)),
        automount: u64_at(STX_ATTRIBUTES) & u64_at(STX_ATTRIBUTES_MASK) & STATX_ATTR_AUTOMOUNT != 0,
        kind: u16::from_ne_bytes([bytes[STX_MODE], bytes[STX_MODE + 1]]) & S_IFMT,
    })
}

// O_PATH without O_DIRECTORY never triggers the final automount (O_DIRECTORY would); the checks below refuse the rest.
pub(super) fn open_same(path: &Path, expected: Identity) -> Option<File> {
    let pinned = OpenOptions::new()
        .read(true)
        .custom_flags(O_PATH | crate::oflags::O_NOFOLLOW)
        .open(path)
        .ok()?;
    let entry = of(&pinned)?;
    if entry.automount || !entry.is_dir() || entry.identity() != expected {
        return None;
    }
    Some(pinned)
}

#[cfg(test)]
#[path = "dirsizestat_tests.rs"]
mod tests;

#[cfg(test)]
pub(super) mod fake {
    use super::*;
    use std::{cell::RefCell, path::PathBuf};

    type Mount = (PathBuf, bool, Option<u64>);
    thread_local! {
        static MOUNT: RefCell<Option<Mount>> = const { RefCell::new(None) };
    }

    pub struct Guard(Option<Mount>);
    impl Drop for Guard {
        fn drop(&mut self) {
            MOUNT.with(|mount| *mount.borrow_mut() = self.0.take());
        }
    }

    pub fn mount(path: &Path, automount: bool, id: Option<u64>) -> Guard {
        Guard(MOUNT.with(|mount| mount.replace(Some((path.to_path_buf(), automount, id)))))
    }

    pub fn apply(path: &Path, mut entry: Entry) -> Entry {
        MOUNT.with(|mount| {
            if let Some((at, automount, id)) = &*mount.borrow() {
                if path == at {
                    entry.automount = *automount;
                    entry.mount = id.or(entry.mount);
                }
            }
        });
        entry
    }

    pub fn apply_held(dir: &File, entry: Entry) -> Entry {
        if MOUNT.with(|mount| mount.borrow().is_some()) {
            if let Ok(path) = std::fs::read_link(held_path(dir)) {
                return apply(&path, entry);
            }
        }
        entry
    }

    pub fn apply_child(dir: &File, name: &OsStr, entry: Entry) -> Entry {
        if MOUNT.with(|mount| mount.borrow().is_some()) {
            if let Ok(path) = std::fs::read_link(held_path(dir)) {
                return apply(&path.join(name), entry);
            }
        }
        entry
    }
}
