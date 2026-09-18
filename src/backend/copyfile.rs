// The copy primitives every transfer is built from: streaming, symlink-preserving, and refusing to overwrite.
use crate::error::{from_io, FleaError};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

// Big enough that the syscall count stops mattering, small enough that a cancel is noticed promptly.
const CHUNK: usize = 256 * 1024;
// rename(2) sets EXDEV when the two paths are on different filesystems, which is the one failure that means "copy instead".
const EXDEV: i32 = 18;
use crate::oflags::{O_DIRECTORY, O_NOFOLLOW};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::io::AsRawFd;

// What a copy reports as it runs; a directory has no total without a sweep, so it reports 0 and renders indeterminate.
pub struct Progress<'a> {
    pub cancel: &'a AtomicBool,
    pub on_bytes: &'a mut dyn FnMut(u64, u64),
    // Some once the copy is inside a directory tree, holding the bytes its earlier files already copied, so a tree reports one running count and no total: the size of a tree is not known without a sweep.
    pub tree: Option<u64>,
    // The destination a copy created and then failed to finish for a reason other than a cancel. It
    // stays on disk, because removing it would destroy data on a transient error, and the caller
    // journals it so undo removes it as one step. A cancel never sets it: the cancel path removes.
    pub partial: Option<PathBuf>,
}

pub fn cancelled(p: &Progress) -> bool {
    p.cancel.load(Ordering::Relaxed)
}

// A path the filesystem is asked about, beside the path an error names. Inside a tree the first is a
// held descriptor's own /proc entry, which is the one parent a rename cannot reach.
#[derive(Clone, Copy)]
pub struct At<'a> {
    pub at: &'a Path,
    pub named: &'a Path,
}

fn here(path: &Path) -> At<'_> {
    At { at: path, named: path }
}

// Copies one regular file, creating the destination exclusively so an existing file is never destroyed.
// Test only: copy_any routes the product's copies, and copynode's fifo test is the last caller by path.
#[cfg(test)]
pub fn copy_file(src: &Path, dst: &Path, total: u64, p: &mut Progress) -> Result<(), FleaError> {
    copy_file_at(here(src), here(dst), total, p)
}

fn copy_file_at(src: At, dst: At, total: u64, p: &mut Progress) -> Result<(), FleaError> {
    // Anything reaching here that is not a regular file was swapped in after copy_any's stat:
    // O_NOFOLLOW refuses a symlink, and regfile's non-blocking open and fstat refuse every other kind.
    let mut r = crate::backend::regfile::open_if_regular(src.at, O_NOFOLLOW)
        .map_err(|e| from_io("copy", &src.named.to_string_lossy(), &e))?;
    // Issue 109: a create takes the umask, so a 0600 source landed 0644 and the copy published what
    // the original kept private. The source's own bits are carried by the create itself, so there is
    // no window where the bytes are on disk under a wider mode, narrowed by the umask and never widened.
    let mode = r.metadata().map(|m| keep_mode(m.permissions().mode())).unwrap_or(0o600);
    let mut w = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(dst.at)
        .map_err(|e| from_io("copy", &dst.named.to_string_lossy(), &e))?;
    // From here the destination exists, and every failure below leaves it for the caller to journal.
    let mut buf = vec![0u8; CHUNK];
    let mut done: u64 = 0;
    loop {
        if cancelled(p) {
            // The partial file goes with the cancel: a half-written destination is not a result anyone asked for.
            drop(w);
            let _ = std::fs::remove_file(dst.at);
            return Err(cancel_err(dst.named));
        }
        let n = match r.read(&mut buf) {
            Ok(n) => n,
            Err(e) => return Err(left_partial(p, dst.named, from_io("copy", &src.named.to_string_lossy(), &e))),
        };
        if n == 0 {
            break;
        }
        if let Err(e) = w.write_all(&buf[..n]) {
            return Err(left_partial(p, dst.named, from_io("copy", &dst.named.to_string_lossy(), &e)));
        }
        done += n as u64;
        let (reported, against) = match p.tree {
            Some(carried) => (carried + done, 0),
            None => (done, total),
        };
        (p.on_bytes)(reported, against);
    }
    if let Err(e) = w.flush() {
        return Err(left_partial(p, dst.named, from_io("copy", &dst.named.to_string_lossy(), &e)));
    }
    if let Some(carried) = p.tree.as_mut() {
        *carried += done;
    }
    Ok(())
}

// The permission bits a copy carries: the source's own, minus anything the umask withholds, and never
// setuid, setgid or the sticky bit, which belong to the file somebody installed and not to its copy.
pub fn keep_mode(mode: u32) -> u32 {
    mode & 0o777 & !umask()
}

// Sample input, one line of /proc/self/status: "Umask:	0022". Read once, because a copy asks per file
// and per directory and this process cannot change its own umask while one runs.
fn umask() -> u32 {
    static READ: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *READ.get_or_init(|| {
        let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
        for line in status.lines() {
            if let Some(value) = line.strip_prefix("Umask:") {
                if let Ok(bits) = u32::from_str_radix(value.trim(), 8) {
                    return bits & 0o777;
                }
            }
        }
        0o022
    })
}

// A failure after the destination was created, and not a cancel: the partial stays, and is reported for the journal.
fn left_partial(p: &mut Progress, dst: &Path, e: FleaError) -> FleaError {
    p.partial = Some(dst.to_path_buf());
    e
}

// A symlink is copied as a symlink and never followed, matching cp -a and every rival in the parity audit.
fn copy_symlink_at(src: At, dst: At) -> Result<(), FleaError> {
    let target = std::fs::read_link(src.at).map_err(|e| from_io("copy", &src.named.to_string_lossy(), &e))?;
    std::os::unix::fs::symlink(&target, dst.at).map_err(|e| from_io("copy", &dst.named.to_string_lossy(), &e))
}

// Copies a file, a symlink, a whole directory tree, or any other node by recreating it. The
// destination must not already exist.
pub fn copy_any(src: &Path, dst: &Path, p: &mut Progress) -> Result<(), FleaError> {
    copy_at(here(src), here(dst), p)
}

fn copy_at(src: At, dst: At, p: &mut Progress) -> Result<(), FleaError> {
    let meta = src
        .at
        .symlink_metadata()
        .map_err(|e| from_io("copy", &src.named.to_string_lossy(), &e))?;
    if meta.file_type().is_symlink() {
        return copy_symlink_at(src, dst);
    }
    if meta.is_dir() {
        return copy_dir_at(src, dst, p);
    }
    if meta.is_file() {
        return copy_file_at(src, dst, meta.len(), p);
    }
    // A fifo, a socket and a device node are the rest, and none of them has contents copy_file could
    // stream: the fifo's open waits, the socket's fails, and the device's would never end.
    crate::backend::copynode::copy_node(&meta, dst.at)
}

fn copy_dir_at(src: At, dst: At, p: &mut Progress) -> Result<(), FleaError> {
    // Issue 110: both ends are held open and every child is reached through those descriptors, because
    // resolving a child from its path again lets a parent renamed aside mid-copy redirect the rest of
    // the tree through a symlink. corner: three descriptors a level, the two ends and the read_dir on
    // the source, so a deep enough tree meets this process's open-file limit where it used to recurse.
    let from = open_dir(src.at).map_err(|e| from_io("copy", &src.named.to_string_lossy(), &e))?;
    // Issue 109 again, one level up: a 0700 directory landed 0755 and its contents were readable by
    // anyone while the copy ran. It is created with nothing the source does not grant and with the
    // owner's own three bits, which this run needs to write into it, and takes its exact mode at the end.
    let keep = from.metadata().ok().map(|m| keep_mode(m.permissions().mode()));
    std::fs::DirBuilder::new().mode(keep.unwrap_or(0o700) | 0o700).create(dst.at)
        .map_err(|e| from_io("copy", &dst.named.to_string_lossy(), &e))?;
    let into = open_dir(dst.at).map_err(|e| from_io("copy", &dst.named.to_string_lossy(), &e))?;
    let (from_held, into_held) = (held_path(&from), held_path(&into));
    // Set once at the top of the tree, so a directory inside it goes on counting rather than starting again.
    if p.tree.is_none() {
        p.tree = Some(0);
    }
    let r = copy_dir_entries(
        At { at: &from_held, named: src.named },
        At { at: &into_held, named: dst.named },
        p,
    );
    if r.is_err() {
        if cancelled(p) {
            // The tree goes with the cancel, the same rule copy_file already applies to a partial file: a
            // half-copied directory is not a result anyone asked for, and no journal step records one.
            // Gated on the flag rather than the message, because a nested copy_file returns its own cancel.
            p.partial = match remove_tree(dst.at, &into) {
                Ok(()) => None,
                // Still there, so the journal is told where it is rather than that nothing was left.
                Err(()) => Some(dst.named.to_path_buf()),
            };
        } else {
            // Any other failure leaves what was copied, since removing it would destroy data on a
            // transient error, and reports the whole tree as the one partial the journal records.
            p.partial = Some(dst.named.to_path_buf());
            // The directory's own change time has to be the newest thing in it: undo reads anything
            // inside it newer than the directory as added since and leaves the tree in place, and the
            // last file's write lands a clock tick after the create that last moved the directory
            // often enough to see. The mode is the one this run made, writable by its owner, so undo
            // can still take the tree away; only the change time moves.
            let _ = std::fs::set_permissions(&into_held, std::fs::Permissions::from_mode(keep.unwrap_or(0o700) | 0o700));
        }
        return r;
    }
    // Last, so a directory this run still has to write into is not made unwritable halfway through.
    // corner: a destination with no mode bits of its own refuses this and keeps the source's bits
    // widened by the owner's three, because a copy that carried every byte is not a failure.
    if let Some(mode) = keep {
        let _ = std::fs::set_permissions(&into_held, std::fs::Permissions::from_mode(mode));
    }
    r
}

fn copy_dir_entries(src: At, dst: At, p: &mut Progress) -> Result<(), FleaError> {
    let entries = std::fs::read_dir(src.at).map_err(|e| from_io("copy", &src.named.to_string_lossy(), &e))?;
    for entry in entries {
        if cancelled(p) {
            return Err(cancel_err(dst.named));
        }
        let entry = entry.map_err(|e| from_io("copy", &src.named.to_string_lossy(), &e))?;
        let name = entry.file_name();
        let (from_at, from_named) = (src.at.join(&name), src.named.join(&name));
        let (into_at, into_named) = (dst.at.join(&name), dst.named.join(&name));
        copy_at(
            At { at: &from_at, named: &from_named },
            At { at: &into_at, named: &into_named },
            p,
        )?;
    }
    Ok(())
}

// A copy of a 0500 source is itself 0500, and nothing can be removed from one. The owner's bits go
// back on only after a removal has actually failed, so a tree without such a directory pays nothing.
fn remove_tree(at: &Path, held: &std::fs::File) -> Result<(), ()> {
    if std::fs::remove_dir_all(at).is_ok() {
        return Ok(());
    }
    owner_can_write(held);
    std::fs::remove_dir_all(at).map_err(|_| ())
}

// Issue 110's discipline again: every child is opened O_NOFOLLOW and reached through this process's
// own descriptor, so a directory swapped for a symlink cannot take the owner's bits somewhere else.
fn owner_can_write(dir: &std::fs::File) {
    let held = held_path(dir);
    let _ = std::fs::set_permissions(&held, std::fs::Permissions::from_mode(0o700));
    let entries = match std::fs::read_dir(&held) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        if let Ok(child) = open_dir(&entry.path()) {
            owner_can_write(&child);
        }
    }
}

// The path that reaches a held directory through this process's own descriptor table, so no rename of
// the name it was opened under can put anything else behind it. Linux only, the one platform this ships on.
fn held_path(dir: &std::fs::File) -> PathBuf {
    PathBuf::from(format!("/proc/self/fd/{}", dir.as_raw_fd()))
}

// O_DIRECTORY refuses anything that is not a directory and O_NOFOLLOW refuses a symlink swapped in at
// the name itself, so the descriptor is the directory this copy stat'd or the open fails.
fn open_dir(path: &Path) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(O_DIRECTORY | O_NOFOLLOW)
        .open(path)
}

// Same filesystem is a rename; a different one is copy-then-remove, and the source only goes once the copy is complete.
pub fn move_any(src: &Path, dst: &Path, p: &mut Progress) -> Result<(), FleaError> {
    match crate::backend::renamecompat::rename_noreplace(src, dst) {
        Ok(()) => Ok(()),
        Err(e) if e.raw_os_error() == Some(EXDEV) => {
            copy_any(src, dst, p)?;
            remove_any(src)
        }
        Err(e) => Err(from_io("rename", &dst.to_string_lossy(), &e)),
    }
}

pub fn remove_any(path: &Path) -> Result<(), FleaError> {
    let meta = path
        .symlink_metadata()
        .map_err(|e| from_io("move", &path.to_string_lossy(), &e))?;
    let r = if meta.is_dir() && !meta.file_type().is_symlink() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    r.map_err(|e| from_io("move", &path.to_string_lossy(), &e))
}

fn cancel_err(path: &Path) -> FleaError {
    FleaError {
        where_: "copy".to_string(),
        path: path.to_string_lossy().to_string(),
        msg: "cancelled".to_string(),
    }
}

#[cfg(test)]
#[path = "copyfile_tests.rs"]
mod tests;
