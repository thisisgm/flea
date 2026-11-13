// A copied file or new folder takes its name only once whole, so no name holds a partial; see copysweep.rs for leftovers.
use std::ffi::CString;
use std::fs::File;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

pub(crate) const PREFIX: &str = ".flea-part-";
// FUSE_SUPER_MAGIC: a daemon's rename may be refused, copied on the device or re-uploaded, so FUSE writes in place.
const FUSE_MAGIC: i64 = 0x65735546;
// An inode with no name until linkat gives it one, so a kill or a crash leaves nothing behind.
pub(super) const O_TMPFILE: i32 = 0o20000000 | crate::oflags::O_DIRECTORY;
// What a filesystem without O_TMPFILE answers: EOPNOTSUPP, or EISDIR and EINVAL from a kernel older than the flag.
pub(super) const NO_TMPFILE: [i32; 3] = [95, 21, 22];
const AT_FDCWD: i32 = -100;
const AT_SYMLINK_FOLLOW: i32 = 0x400;
// A stale sibling at the next name (a dead process that had this pid) is passed over, a few times at most.
const NAME_TRIES: u32 = 8;
const EEXIST: i32 = 17;
const EINVAL: i32 = 22;
static NEXT: AtomicU64 = AtomicU64::new(0);
// The filesystem magics whose first O_TMPFILE create was refused; they stage through a named sibling from then on.
static NO_UNNAMED: Mutex<Vec<i64>> = Mutex::new(Vec::new());

extern "C" {
    fn linkat(olddirfd: i32, oldpath: *const std::ffi::c_char, newdirfd: i32, newpath: *const std::ffi::c_char, flags: i32) -> i32;
}

#[derive(PartialEq)]
enum Held {
    InPlace,
    Sibling,
    Unnamed,
}

// Where one file's bytes go: an unnamed inode, a hidden sibling, or the name itself where the copy writes in place.
pub(crate) struct Stage {
    at: PathBuf,
    held: Held,
}

impl Stage {
    // Creates the file exclusively with the source's mode; inside a folder this copy staged, the folder hides it already.
    pub(crate) fn create(dst: &Path, magic: Option<i64>, mode: u32, inside: bool) -> io::Result<(Stage, File)> {
        let open = |at: &Path| std::fs::OpenOptions::new().write(true).create_new(true).mode(mode).open(at);
        if inside || !stages(magic) {
            return open(dst).map(|file| (Stage { at: dst.to_path_buf(), held: Held::InPlace }, file));
        }
        let parent = dst.parent().unwrap_or(Path::new("/"));
        if let Some(file) = unnamed(parent, magic, mode)? {
            let at = crate::backend::copyfile::held_path(&file);
            return Ok((Stage { at, held: Held::Unnamed }, file));
        }
        let (at, file) = sibling(parent, open)?;
        Ok((Stage { at, held: Held::Sibling }, file))
    }

    // The path that reaches the file: its name, its sibling's, or its own descriptor's /proc entry.
    pub(crate) fn at(&self) -> &Path {
        &self.at
    }

    pub(crate) fn staged(&self) -> bool {
        self.held != Held::InPlace
    }

    // linkat and the no-clobber rename both answer EEXIST for a name another writer took, as the exclusive create did.
    pub(crate) fn publish(&self, dst: &Path) -> io::Result<()> {
        match self.held {
            Held::InPlace => Ok(()),
            Held::Unnamed => failed_publish().map_or_else(|| link(&self.at, dst), Err),
            Held::Sibling => failed_publish().map_or_else(|| crate::backend::renamecompat::rename_noreplace(&self.at, dst), Err),
        }
    }

    // An unnamed inode goes when its descriptor closes, so only a name needs removing.
    pub(crate) fn discard(&self) -> io::Result<()> {
        if self.held == Held::Unnamed {
            return Ok(());
        }
        #[cfg(test)]
        if let Some(errno) = FAIL_DISCARD.with(|v| v.take()) {
            return Err(io::Error::from_raw_os_error(errno));
        }
        std::fs::remove_file(&self.at)
    }
}

// A new folder and everything in it are built under a hidden sibling, renamed onto the name once whole.
pub(crate) struct Folder {
    at: PathBuf,
    staged: bool,
}

impl Folder {
    // Inside a folder this copy staged, or on a destination that writes in place, the folder takes its name at once.
    pub(crate) fn create(dst: &Path, mode: u32, inside: bool) -> io::Result<Folder> {
        let make = |at: &Path| std::fs::DirBuilder::new().mode(mode).create(at);
        let parent = dst.parent().unwrap_or(Path::new("/"));
        if inside || !stages(crate::backend::fsinfo::magic_of(parent)) {
            return make(dst).map(|()| Folder { at: dst.to_path_buf(), staged: false });
        }
        let (at, ()) = sibling(parent, make)?;
        Ok(Folder { at, staged: true })
    }

    pub(crate) fn at(&self) -> &Path {
        &self.at
    }

    pub(crate) fn staged(&self) -> bool {
        self.staged
    }

    pub(crate) fn publish(&self, dst: &Path) -> io::Result<()> {
        if !self.staged {
            return Ok(());
        }
        failed_publish().map_or_else(|| crate::backend::renamecompat::rename_noreplace(&self.at, dst), Err)
    }
}

pub(crate) fn stages(magic: Option<i64>) -> bool {
    #[cfg(test)]
    if IN_PLACE.with(|v| v.get()) {
        return false;
    }
    // A filesystem statfs could not name writes in place too, as before.
    magic.is_some_and(|m| m != FUSE_MAGIC)
}

// The first create on each filesystem is its probe, and a refusal is remembered for this process.
fn unnamed(parent: &Path, magic: Option<i64>, mode: u32) -> io::Result<Option<File>> {
    let magic = magic.unwrap_or_default();
    if NO_UNNAMED.lock().unwrap_or_else(|e| e.into_inner()).contains(&magic) {
        return Ok(None);
    }
    #[cfg(test)]
    UNNAMED_TRIES.with(|v| v.set(v.get() + 1));
    #[cfg(test)]
    let refused = REFUSE_UNNAMED.with(|v| v.get()).map(io::Error::from_raw_os_error);
    #[cfg(not(test))]
    let refused: Option<io::Error> = None;
    let opened = match refused {
        Some(e) => Err(e),
        None => std::fs::OpenOptions::new().write(true).mode(mode).custom_flags(O_TMPFILE).open(parent),
    };
    match opened {
        Ok(file) => Ok(Some(file)),
        Err(e) if e.raw_os_error().is_some_and(|code| NO_TMPFILE.contains(&code)) => {
            NO_UNNAMED.lock().unwrap_or_else(|e| e.into_inner()).push(magic);
            Ok(None)
        }
        Err(e) => Err(e),
    }
}

// Names an O_TMPFILE inode through its descriptor's /proc entry, the unprivileged form of linkat.
fn link(fd_path: &Path, dst: &Path) -> io::Result<()> {
    let c = |path: &Path| CString::new(path.as_os_str().as_bytes()).map_err(|_| io::Error::from_raw_os_error(EINVAL));
    let (from, to) = (c(fd_path)?, c(dst)?);
    if unsafe { linkat(AT_FDCWD, from.as_ptr(), AT_FDCWD, to.as_ptr(), AT_SYMLINK_FOLLOW) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

fn sibling<T>(parent: &Path, make: impl Fn(&Path) -> io::Result<T>) -> io::Result<(PathBuf, T)> {
    let mut tries = 0;
    loop {
        let at = parent.join(leaf(NEXT.fetch_add(1, Ordering::Relaxed)));
        match make(&at) {
            Ok(made) => return Ok((at, made)),
            Err(e) if e.raw_os_error() == Some(EEXIST) && tries < NAME_TRIES => tries += 1,
            Err(e) => return Err(e),
        }
    }
}

// Sample output: ".flea-part-3f2a9c1e-48211-7", the boot, this process and its seventh file or folder.
fn leaf(seq: u64) -> String {
    format!("{PREFIX}{}-{}-{seq}", boot(), std::process::id())
}

// Sample input: "3f2a9c1e-5b7d-4c1a-9e2f-0a1b2c3d4e5f\n" gives "3f2a9c1e"; a box without one still names its files uniquely by pid.
pub(crate) fn boot() -> &'static str {
    static BOOT: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
        let id = std::fs::read_to_string("/proc/sys/kernel/random/boot_id").unwrap_or_default();
        let hex: String = id.chars().filter(char::is_ascii_hexdigit).take(8).collect();
        if hex.len() == 8 { hex.to_ascii_lowercase() } else { "00000000".to_string() }
    });
    &BOOT
}

fn failed_publish() -> Option<io::Error> {
    #[cfg(test)]
    if let Some(errno) = FAIL_PUBLISH.with(|v| v.take()) {
        return Some(io::Error::from_raw_os_error(errno));
    }
    None
}

#[cfg(test)]
thread_local! {
    static IN_PLACE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static FAIL_PUBLISH: std::cell::Cell<Option<i32>> = const { std::cell::Cell::new(None) };
    static FAIL_DISCARD: std::cell::Cell<Option<i32>> = const { std::cell::Cell::new(None) };
    static REFUSE_UNNAMED: std::cell::Cell<Option<i32>> = const { std::cell::Cell::new(None) };
    static UNNAMED_TRIES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

// True makes every destination on this thread write in place, as a FUSE mount does.
#[cfg(test)]
pub(crate) fn test_in_place(on: bool) {
    IN_PLACE.with(|v| v.set(on));
}

// The next publish on this thread answers this errno instead of linking or renaming.
#[cfg(test)]
pub(crate) fn test_fail_publish(errno: Option<i32>) {
    FAIL_PUBLISH.with(|v| v.set(errno));
}

// The next discard on this thread answers this errno and leaves the sibling where it is.
#[cfg(test)]
pub(crate) fn test_fail_discard(errno: Option<i32>) {
    FAIL_DISCARD.with(|v| v.set(errno));
}

// Every O_TMPFILE create on this thread answers this errno, as vfat (EOPNOTSUPP) does.
#[cfg(test)]
pub(crate) fn test_refuse_unnamed(errno: Option<i32>) {
    REFUSE_UNNAMED.with(|v| v.set(errno));
}

// The O_TMPFILE creates this thread tried since the last read.
#[cfg(test)]
pub(crate) fn test_take_unnamed_tries() -> usize {
    UNNAMED_TRIES.with(|v| v.replace(0))
}

#[cfg(test)]
pub(crate) fn test_leaf_for(pid: u32, seq: u64) -> String {
    format!("{PREFIX}{}-{pid}-{seq}", boot())
}
