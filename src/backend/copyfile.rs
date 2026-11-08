// The copy primitives every transfer is built from: streaming, symlink-preserving, and refusing to overwrite.
use crate::error::{from_io, FleaError};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

// 8 MiB is 32 chunks, so one range sync per 32 writes keeps the card live without a syscall per chunk.
pub(crate) const CONFIRM_BYTES: u64 = 8 * 1024 * 1024;
// 1 MiB starts the ramp, so the first confirmed report lands after 1 MiB rather than 16 MiB.
pub(crate) const FIRST_CONFIRM_BYTES: u64 = 1024 * 1024;
// Linux statfs magic for vfat, whose files hold at most 4 GiB minus one byte; see linux/magic.h.
const VFAT_MAGIC: i64 = 0x4D44;
// A file of 4 GiB or more never starts onto vfat.
const FOUR_GIB: u64 = 4 * 1024 * 1024 * 1024;
// rename(2) sets EXDEV when the two paths are on different filesystems, which is the one failure that means "copy instead".
const EXDEV: i32 = 18;
use crate::oflags::{O_DIRECTORY, O_NOFOLLOW};
use super::archivefree::format_size;
use super::unheld::Kind;
use super::copystage::{Folder, Stage};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::io::AsRawFd;
#[cfg(test)]
use super::copyread::CHUNK;

// What a copy reports as it runs; a directory has no total without a sweep, so it reports 0 and renders indeterminate.
pub struct Progress<'a> {
    pub cancel: &'a AtomicBool,
    pub on_bytes: &'a mut dyn FnMut(u64, u64),
    // Some once the copy is inside a directory tree, holding the bytes its earlier files already copied, so a tree reports one running count and no total: the size of a tree is not known without a sweep.
    pub tree: Option<u64>,
    // A tree, an in-place file or an unremovable sibling a non-cancel failure left, journaled so undo removes it.
    pub partial: Option<PathBuf>,
    // Where a tree copy records every path it creates; a finished copy drops it unread.
    pub manifest: Option<super::copymanifest::Writer>,
    // Some while the destination needs its bytes confirmed: transfer, duplicate and redo each make one for theirs.
    pub durability: Option<&'a mut super::durable::Durability>,
    // True when the copy is a move's first half, so the sentence naming what a drive refused says moved.
    pub for_move: bool,
}

pub fn cancelled(p: &Progress) -> bool {
    p.cancel.load(Ordering::Relaxed)
}

// Doubling ramp slices cap at CONFIRM_BYTES, so early progress is quick and steady state matches today.
fn next_slice_len(prev: Option<u64>) -> u64 {
    match prev {
        None => FIRST_CONFIRM_BYTES,
        Some(len) => (len * 2).min(CONFIRM_BYTES),
    }
}

// The one durable call every copy path makes: a no-op unless the destination needs confirming.
fn touch(p: &mut Progress, dir: &Path) {
    if let Some(durability) = p.durability.as_mut() {
        durability.touch(dir);
    }
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
    copy_file_at(here(src), here(dst), total, p, false)
}

fn copy_file_at(src: At, dst: At, total: u64, p: &mut Progress, inside: bool) -> Result<(), FleaError> {
    // Anything reaching here that is not a regular file was swapped in after copy_any's stat:
    // O_NOFOLLOW refuses a symlink, and regfile's non-blocking open and fstat refuse every other kind.
    let (r, src_meta) = crate::backend::regfile::open_if_regular_with_meta(src.at, O_NOFOLLOW)
        .map_err(|e| from_io("copy", &src.named.to_string_lossy(), &e))?;
    // A file the destination cannot hold is refused before a byte is written, so EFBIG never leaves a partial.
    let dest_magic = match precheck(dst, src_meta.len()) {
        Ok(magic) => magic,
        // The size limit belongs to this one file, the free room to the whole drive.
        Err(refusal) => return Err(if refusal == TOO_BIG { super::unheld::marked(dst.named, refusal) } else { copy_err(dst.named, &refusal) }),
    };
    // Issue 109: a create takes the umask, so a 0600 source landed 0644 and the copy published what
    // the original kept private. The source's own bits are carried by the create itself, so there is
    // no window where the bytes are on disk under a wider mode, narrowed by the umask and never widened.
    let source_magic = crate::backend::fsinfo::magic_of(src.at);
    let mode = mode_for_source(source_magic, src_meta.permissions().mode(), false);
    let mut r = super::copyread::Reader::new(r, src_meta.len(), source_magic)
        .map_err(|e| from_io("copy", &src.named.to_string_lossy(), &e))?;
    // The bytes go to a hidden sibling, which takes the name only once they are whole; see copystage.rs.
    let (stage, mut w) = super::unheld::fault(dst.named, Kind::Entry)
        .and_then(|()| Stage::create(dst.at, dest_magic, mode, inside))
        .map_err(|e| super::unheld::refused(Kind::Entry, &e, dst, p.tree.is_some().then_some(src.at)))?;
    #[cfg(test)]
    super::durable::test_log("open-dest");
    // From here the file exists: a cancel discards it, and every other failure goes through left_half_written or publish.
    let durable = p.durability.as_ref().is_some_and(|c| c.durable);
    let mut done: u64 = 0;
    let mut confirmed: u64 = 0;
    // A filesystem without range writeback (some FUSE mounts answer EINVAL) leaves the verdict to the final fsync.
    let mut slicing = durable;
    // Offset of the slice whose writeback started but whose wait is still owed, with its length.
    let mut inflight: Option<(u64, u64)> = None;
    loop {
        if cancelled(p) {
            // The partial file goes with the cancel: a half-written destination is not a result anyone asked for.
            drop(w);
            let _ = stage.discard();
            return Err(cancel_err(dst.named));
        }
        let read = r.read(p.cancel);
        if cancelled(p) {
            continue;
        }
        let n = match read {
            Ok(n) => n,
            Err(e) => return Err(left_half_written(p, dst, &stage, &w, from_io("copy", &src.named.to_string_lossy(), &e))),
        };
        if n == 0 {
            break;
        }
        if let Err(e) = write_chunk(&mut w, &r.buffer[..n]) {
            return Err(left_half_written(p, dst, &stage, &w, from_io("copy", &dst.named.to_string_lossy(), &e)));
        }
        #[cfg(test)]
        super::durable::test_log("write-dest");
        #[cfg(test)]
        if CANCEL_AFTER_WRITE.with(|v| v.get()) {
            p.cancel.store(true, Ordering::Relaxed);
        }
        done += n as u64;
        if !durable {
            let (reported, against) = match p.tree {
                Some(carried) => (carried + done, 0),
                None => (done, total),
            };
            (p.on_bytes)(reported, against);
        } else {
            // Pipelined writeback: slice k starts with WRITE alone, then slice k-1 waits and reports.
            loop {
                let next = inflight.map(|(off, len)| off + len).unwrap_or(confirmed);
                let len = next_slice_len(inflight.map(|(_, len)| len));
                if !slicing || done < next + len {
                    break;
                }
                if let Err(e) = crate::backend::durable::sync_range_write(&w, next, len) {
                    if crate::backend::durable::range_unsupported(&e) {
                        slicing = false;
                        inflight = None;
                        break;
                    }
                    if let Some(durability) = p.durability.as_mut() {
                        durability.note_file_failed();
                    }
                    return Err(left_half_written(p, dst, &stage, &w, from_io("copy", &dst.named.to_string_lossy(), &e)));
                }
                if let Some((prev_off, prev_len)) = inflight {
                    if let Err(e) = crate::backend::durable::sync_range(&w, prev_off, prev_len) {
                        if crate::backend::durable::range_unsupported(&e) {
                            slicing = false;
                            inflight = None;
                            break;
                        }
                        if let Some(durability) = p.durability.as_mut() {
                            durability.note_file_failed();
                        }
                        return Err(left_half_written(p, dst, &stage, &w, from_io("copy", &dst.named.to_string_lossy(), &e)));
                    }
                    confirmed = prev_off + prev_len;
                    let (reported, against) = match p.tree {
                        Some(carried) => (carried + confirmed, 0),
                        None => (confirmed, total),
                    };
                    (p.on_bytes)(reported, against);
                }
                inflight = Some((next, len));
            }
        }
    }
    if let Err(e) = w.flush() {
        return Err(left_half_written(p, dst, &stage, &w, from_io("copy", &dst.named.to_string_lossy(), &e)));
    }
    // The copy carries the source mtime, best effort, so a copied tree still sorts by its own history.
    keep_mtime(&w, &src_meta);
    super::tags::copy_tags_onto(src.at, &w);
    if !durable {
        publish(p, src, dst, &stage)?;
    }
    if durable {
        // A batch_syncfs stick confirms 64 files with one syncfs, so a small file skips its own fsync.
        if p.durability.as_ref().is_some_and(|d| d.batch_syncfs) {
            if inflight.is_some() {
                if let Err(e) = crate::backend::durable::fsync_file(&w) {
                    if let Some(durability) = p.durability.as_mut() {
                        durability.note_file_failed();
                    }
                    return Err(left_half_written(p, dst, &stage, &w, from_io("copy", &dst.named.to_string_lossy(), &e)));
                }
            }
            publish(p, src, dst, &stage)?;
            // The bytes count when written, at most one batch ahead of the drive's own syncfs.
            report_landed(p, dst, done, total);
            if let Some(carried) = p.tree.as_mut() {
                *carried += done;
            }
            record_open(p, dst.named, &w);
            if let Some(durability) = p.durability.as_mut() {
                durability.hold(w);
            }
            return Ok(());
        }
        // The bytes count only once the drive confirms them, so the rate is the drive's real rate.
        if let Err(e) = crate::backend::durable::fsync_file(&w) {
            if let Some(durability) = p.durability.as_mut() {
                durability.note_file_failed();
            }
            return Err(left_half_written(p, dst, &stage, &w, from_io("copy", &dst.named.to_string_lossy(), &e)));
        }
        publish(p, src, dst, &stage)?;
        report_landed(p, dst, done, total);
    }
    if let Some(carried) = p.tree.as_mut() {
        *carried += done;
    }
    record_open(p, dst.named, &w);
    Ok(())
}

// A tree copy reports its running total and no file size, so the bar tracks the whole tree.
fn report_landed(p: &mut Progress, dst: At, done: u64, total: u64) {
    let (reported, against) = match p.tree {
        Some(carried) => (carried + done, 0),
        None => (done, total),
    };
    (p.on_bytes)(reported, against);
    if let Some(parent) = dst.named.parent() {
        touch(p, parent);
    }
}

#[cfg(test)]
thread_local! {
    static CANCEL_AFTER_WRITE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

// Sample input: test_cancel_after_write(true) raises the copy's own cancel after its first chunk lands; false clears it.
#[cfg(test)]
pub(crate) fn test_cancel_after_write(on: bool) {
    CANCEL_AFTER_WRITE.with(|v| v.set(on));
}

#[cfg(test)]
pub(super) use super::copyread::test_take_buffers;

// Production writes every byte; tests can refuse after a partial write without a sick drive.
fn write_chunk(file: &mut std::fs::File, bytes: &[u8]) -> std::io::Result<()> {
    #[cfg(test)]
    if let Some(errno) = TEST_WRITE_FAILURE.with(|v| {
        let mut fault = v.borrow_mut();
        match fault.as_mut() {
            Some((remaining, errno)) if *remaining == 0 => Some(*errno),
            Some((remaining, _)) => { *remaining -= 1; None }
            None => None,
        }
    }) {
        const PARTIAL_DIVISOR: usize = 2;
        file.write_all(&bytes[..bytes.len() / PARTIAL_DIVISOR])?;
        return Err(if errno == 0 { std::io::ErrorKind::WriteZero.into() }
            else { std::io::Error::from_raw_os_error(errno) });
    }
    file.write_all(bytes)
}

#[cfg(test)]
thread_local! {
    static TEST_WRITE_FAILURE: std::cell::RefCell<Option<(usize, i32)>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
thread_local! {
    static STUCK_FOLDER: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

// The next failed staged folder on this thread stays, as one whose removal the drive refuses would.
#[cfg(test)]
pub(crate) fn test_stuck_folder() {
    STUCK_FOLDER.with(|v| v.set(true));
}

// Some((1, ENOSPC)) refuses the second write after writing half its bytes; None restores real writes.
#[cfg(test)]
pub(crate) fn test_fail_write(fault: Option<(usize, i32)>) {
    TEST_WRITE_FAILURE.with(|v| *v.borrow_mut() = fault);
}

// A file the destination cannot hold is refused before a byte is written; otherwise the magic decides the staging.
fn precheck(dst: At, size: u64) -> Result<Option<i64>, String> {
    let name = dst.named.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    #[cfg(test)]
    if let Some((free, blocks, magic)) = TEST_PRECHECK.with(|v| *v.borrow()) {
        return refuse_for_size(&name, magic, free, blocks, size).map_or(Ok(magic), Err);
    }
    // The held path, since inside a staged folder the named parent does not exist until the folder is renamed.
    let parent = dst.at.parent().unwrap_or(dst.at);
    let (info, magic) = crate::backend::fsinfo::read_with_magic(parent);
    let (free, blocks) = info.map(|i| (i.free, i.blocks)).unwrap_or((u64::MAX, 1));
    refuse_for_size(&name, magic, free, blocks, size).map_or(Ok(magic), Err)
}

// Sample input: test_set_precheck(Some(0), Some(0), None) makes the next precheck see unknown room.
#[cfg(test)]
thread_local! {
    static TEST_PRECHECK: std::cell::RefCell<Option<(u64, u64, Option<i64>)>> = const { std::cell::RefCell::new(None) };
}

// A test injects the free figure precheck sees, so copy_any reaches the room refusal with no mount.
#[cfg(test)]
pub fn test_set_precheck(free: Option<u64>, blocks: Option<u64>, magic: Option<i64>) {
    TEST_PRECHECK.with(|v| *v.borrow_mut() = free.map(|f| (f, blocks.unwrap_or(1), magic)));
}

const TOO_BIG: &str = "a file of 4 GiB or more does not fit on a FAT32 drive";

// Sample input: ("a.iso", Some(0x4D44), free, 1, 4 GiB) refuses, ("a.iso", Some(ext4), free, 1, 4 GiB) passes.
fn refuse_for_size(name: &str, magic: Option<i64>, free: u64, blocks: u64, size: u64) -> Option<String> {
    if magic == Some(VFAT_MAGIC) && size >= FOUR_GIB {
        return Some(TOO_BIG.to_string());
    }
    // A zero-block statfs reports unknown room, so a gvfs mount that omits free never refuses.
    if blocks == 0 {
        return None;
    }
    if size > free {
        return Some(format!("{name} needs {}, and the drive has {} free", format_size(size), format_size(free)));
    }
    None
}

fn copy_err(path: &Path, msg: &str) -> FleaError {
    FleaError { where_: "copy".to_string(), path: path.to_string_lossy().to_string(), msg: msg.to_string() }
}

// The permission bits a copy carries: the source's own, minus anything the umask withholds, and never
// setuid, setgid or the sticky bit, which belong to the file somebody installed and not to its copy.
pub fn keep_mode(mode: u32) -> u32 {
    mode & 0o777 & !umask()
}

// A source filesystem with no real modes lends none, so a copy takes the umask default instead of publishing exec bits.
fn mode_for_source(magic: Option<i64>, mode: u32, is_dir: bool) -> u32 {
    const EXFAT: i64 = 0x2011BAB0;
    const NTFS: i64 = 0x5346544E;
    const NTFS3: i64 = 0x7366746e;
    match magic {
        Some(VFAT_MAGIC) | Some(EXFAT) | Some(NTFS) | Some(NTFS3) => {
            if is_dir {
                0o777 & !umask()
            } else {
                0o666 & !umask()
            }
        }
        _ => keep_mode(mode),
    }
}

// One mtime set for the mtime a copy carries, best effort, so a refusing filesystem keeps today's time rather than failing.
fn keep_mtime(w: &std::fs::File, src_meta: &std::fs::Metadata) {
    use std::os::unix::fs::MetadataExt;
    use std::time::{Duration, UNIX_EPOCH};
    let time = if src_meta.mtime() >= 0 {
        UNIX_EPOCH.checked_add(Duration::new(src_meta.mtime() as u64, src_meta.mtime_nsec() as u32))
    } else {
        UNIX_EPOCH.checked_sub(Duration::new(src_meta.mtime().unsigned_abs(), 0))
    };
    if let Some(time) = time {
        #[cfg(test)]
        super::durable::test_log("mtime-dest");
        let _ = w.set_modified(time);
    }
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

// A hidden sibling goes as a cancel's does; an in-place file or a sibling that would not go stays for the journal.
fn left_half_written(p: &mut Progress, dst: At, stage: &Stage, w: &std::fs::File, e: FleaError) -> FleaError {
    if stage.staged() && stage.discard().is_ok() {
        return e;
    }
    let named = match stage.at().file_name() {
        Some(leaf) if stage.staged() => dst.named.with_file_name(leaf),
        _ => dst.named.to_path_buf(),
    };
    record_open(p, &named, w);
    left_partial(p, &named, e)
}

// A staged folder that failed goes whole, so its name never holds part of a tree; one that will not go is journaled.
fn drop_folder(p: &mut Progress, dst: At, folder: &Folder, into: &std::fs::File, e: FleaError) -> FleaError {
    let leaf = folder.at().file_name().map(|leaf| dst.named.with_file_name(leaf));
    #[cfg(test)]
    let removed = if STUCK_FOLDER.with(|v| v.replace(false)) { Err(()) } else { remove_tree(folder.at(), into) };
    #[cfg(not(test))]
    let removed = remove_tree(folder.at(), into);
    p.partial = removed.err().and(leaf);
    // This folder's own manifest names paths that never existed, so undo judges the sibling whole; a merge keeps its records.
    if p.partial.is_some() && p.manifest.as_ref().is_some_and(|m| m.root() == dst.named) { p.manifest = None; }
    if let Some(durability) = p.durability.as_mut() {
        durability.forget_tree(dst.named);
    }
    e
}

// The whole file takes its name; a name taken meanwhile, or one the drive refuses, answers as the create would have.
fn publish(p: &Progress, src: At, dst: At, stage: &Stage) -> Result<(), FleaError> {
    stage.publish(dst.at).map_err(|e| {
        let _ = stage.discard();
        super::unheld::refused(Kind::Entry, &e, dst, p.tree.is_some().then_some(src.at))
    })
}

// The identity is fstat'd off the still-open descriptor, so undo verifies what is on disk without a later sweep.
fn record_open(p: &mut Progress, named: &Path, w: &std::fs::File) {
    if let Some(writer) = p.manifest.as_mut() {
        match w.metadata() {
            Ok(meta) => writer.record(named, &meta),
            Err(_) => writer.overflow(),
        }
    }
}

// A symlink is copied as a symlink and never followed, matching cp -a and every file manager in the parity audit.
fn copy_symlink_at(src: At, dst: At, p: &mut Progress) -> Result<(), FleaError> {
    let target = std::fs::read_link(src.at).map_err(|e| from_io("copy", &src.named.to_string_lossy(), &e))?;
    super::unheld::fault(dst.named, Kind::Link)
        .and_then(|()| std::os::unix::fs::symlink(&target, dst.at))
        .map_err(|e| super::unheld::refused(Kind::Link, &e, dst, p.tree.is_some().then_some(src.at)))?;
    super::tags::copy_tags(src.at, dst.at);
    if let Some(writer) = p.manifest.as_mut() {
        writer.record_stat(dst.at, dst.named);
    }
    if let Some(parent) = dst.named.parent() {
        touch(p, parent);
    }
    Ok(())
}

// Copies a file, a symlink, a whole directory tree, or any other node by recreating it. The
// destination must not already exist.
pub fn copy_any(src: &Path, dst: &Path, p: &mut Progress) -> Result<(), FleaError> {
    super::unheld::begin(p.durability.as_ref().is_some_and(|d| d.rename_fat));
    if let Err(e) = copy_at(here(src), here(dst), p, false) {
        // An item that ends on its own error counts none of its refusals, so none of them reaches the done line.
        super::unheld::take();
        return Err(super::unheld::plain(e));
    }
    // A tree that landed without what the drive refused is a partial copy: journaled as one, and named.
    match super::unheld::summary(dst, p.for_move) {
        Some(e) => {
            p.partial = Some(dst.to_path_buf());
            Err(e)
        }
        None => Ok(()),
    }
}

fn copy_at(src: At, dst: At, p: &mut Progress, inside: bool) -> Result<(), FleaError> {
    let meta = src
        .at
        .symlink_metadata()
        .map_err(|e| from_io("copy", &src.named.to_string_lossy(), &e))?;
    if meta.file_type().is_symlink() {
        return copy_symlink_at(src, dst, p);
    }
    if meta.is_dir() {
        return copy_dir_at(src, dst, p, inside);
    }
    if meta.is_file() {
        return copy_file_at(src, dst, meta.len(), p, inside);
    }
    // A fifo, a socket and a device node are the rest, and none of them has contents copy_file could
    // stream: the fifo's open waits, the socket's fails, and the device's would never end.
    crate::backend::copynode::copy_node(&meta, src.at, dst, p.tree.is_some().then_some(src.at))?;
    if let Some(writer) = p.manifest.as_mut() {
        writer.record_stat(dst.at, dst.named);
    }
    if let Some(parent) = dst.named.parent() {
        touch(p, parent);
    }
    Ok(())
}

fn copy_dir_at(src: At, dst: At, p: &mut Progress, inside: bool) -> Result<(), FleaError> {
    // Issue 110: both ends are held open and every child is reached through those descriptors, because
    // resolving a child from its path again lets a parent renamed aside mid-copy redirect the rest of
    // the tree through a symlink. corner: three descriptors a level, the two ends and the read_dir on
    // the source, so a deep enough tree meets this process's open-file limit where it used to recurse.
    let from = open_dir(src.at).map_err(|e| from_io("copy", &src.named.to_string_lossy(), &e))?;
    // Issue 109 again, one level up: a 0700 directory landed 0755 and its contents were readable by
    // anyone while the copy ran. It is created with nothing the source does not grant and with the
    // owner's own three bits, which this run needs to write into it, and takes its exact mode at the end.
    let source_magic = crate::backend::fsinfo::magic_of(src.at);
    let keep = from.metadata().ok().map(|m| mode_for_source(source_magic, m.permissions().mode(), true));
    // A new folder is built under a hidden sibling and takes its name in one rename; see copystage.rs.
    let folder = super::unheld::fault(dst.named, Kind::Entry)
        .and_then(|()| Folder::create(dst.at, keep.unwrap_or(0o700) | 0o700, inside))
        .map_err(|e| super::unheld::refused(Kind::Entry, &e, dst, p.tree.is_some().then_some(src.at)))?;
    // A staged folder is still empty here, so it goes at once rather than wait for a sweep after this backend.
    let into = open_dir(folder.at()).inspect_err(|_| if folder.staged() { let _ = std::fs::remove_dir(folder.at()); })
        .map_err(|e| from_io("copy", &dst.named.to_string_lossy(), &e))?;
    super::tags::copy_tags(src.at, folder.at());
    if let Some(writer) = p.manifest.as_mut() {
        match into.metadata() {
            Ok(meta) => writer.record(dst.named, &meta),
            Err(_) => writer.overflow(),
        }
    }
    let (from_held, into_held) = (held_path(&from), held_path(&into));
    // Set once at the top of the tree, so a directory inside it goes on counting rather than starting again.
    if p.tree.is_none() {
        p.tree = Some(0);
    }
    let r = copy_dir_entries(
        At { at: &from_held, named: src.named },
        At { at: &into_held, named: dst.named },
        p,
        inside || folder.staged(),
    );
    let r = match r {
        Err(e) if folder.staged() => return Err(drop_folder(p, dst, &folder, &into, e)),
        r => r,
    };
    if r.is_err() {
        if cancelled(p) {
            // A half-copied tree goes with the cancel, gated on the flag not the message.
            p.partial = match remove_tree(dst.at, &into) {
                Ok(()) => None,
                // Still there, so the journal is told where it is rather than that nothing was left.
                Err(()) => Some(dst.named.to_path_buf()),
            };
            if p.partial.is_none() {
                if let Some(durability) = p.durability.as_mut() {
                    durability.forget_tree(dst.named);
                }
            }
        } else {
            // Any other failure leaves what was copied, since removing it would destroy data on a
            // transient error, and reports the whole tree as the one partial the journal records.
            p.partial = Some(dst.named.to_path_buf());
        }
        return r;
    }
    // Last, so a directory this run still has to write into is not made unwritable halfway through.
    // corner: a destination with no mode bits of its own refuses this and keeps the source's bits
    // widened by the owner's three, because a copy that carried every byte is not a failure.
    if let Some(mode) = keep {
        let _ = std::fs::set_permissions(&into_held, std::fs::Permissions::from_mode(mode));
    }
    // The directory carries the source mtime too, after its children landed inside it.
    if let Ok(meta) = from.metadata() {
        keep_mtime(&into, &meta);
    }
    if let Err(e) = folder.publish(dst.at) {
        let refused = super::unheld::refused(Kind::Entry, &e, dst, p.tree.is_some().then_some(src.at));
        return Err(drop_folder(p, dst, &folder, &into, refused));
    }
    touch(p, dst.named);
    if let Some(parent) = dst.named.parent() {
        touch(p, parent);
    }
    r
}

fn copy_dir_entries(src: At, dst: At, p: &mut Progress, inside: bool) -> Result<(), FleaError> {
    let entries = std::fs::read_dir(src.at).map_err(|e| from_io("copy", &src.named.to_string_lossy(), &e))?;
    let rename_fat = p.durability.as_ref().is_some_and(|d| d.rename_fat);
    for entry in entries {
        if cancelled(p) {
            return Err(cancel_err(dst.named));
        }
        let entry = entry.map_err(|e| from_io("copy", &src.named.to_string_lossy(), &e))?;
        let name = entry.file_name();
        // The card's Rename answer reaches every level, so a name the drive refuses lands under the name it gives a top-level one.
        let landed = super::fatnames::inside_leaf(&name, rename_fat);
        let (from_at, from_named) = (src.at.join(&name), src.named.join(&name));
        let (into_at, into_named) = (dst.at.join(&landed), dst.named.join(&landed));
        let outcome = if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            // The total is unused inside a tree: p.tree is Some here.
            copy_file_at(
                At { at: &from_at, named: &from_named },
                At { at: &into_at, named: &into_named },
                0,
                p,
                inside,
            )
        } else {
            copy_at(
                At { at: &from_at, named: &from_named },
                At { at: &into_at, named: &into_named },
                p,
                inside,
            )
        };
        // An item the drive cannot hold is named and skipped, so one link or one FAT name never ends a tree.
        match outcome {
            Err(e) if e.where_ == super::unheld::UNHELD => super::unheld::note(e),
            Err(e) => return Err(e),
            Ok(()) => {}
        }
    }
    Ok(())
}

// A copy of a 0500 source is itself 0500, and nothing can be removed from one. The owner's bits go
// back on only after a removal has actually failed, so a tree without such a directory pays nothing.
pub(super) fn remove_tree(at: &Path, held: &std::fs::File) -> Result<(), ()> {
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
pub(super) fn held_path(dir: &std::fs::File) -> PathBuf {
    PathBuf::from(format!("/proc/self/fd/{}", dir.as_raw_fd()))
}

// O_DIRECTORY refuses anything that is not a directory and O_NOFOLLOW refuses a symlink swapped in at
// the name itself, so the descriptor is the directory this copy stat'd or the open fails.
pub(super) fn open_dir(path: &Path) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(O_DIRECTORY | O_NOFOLLOW)
        .open(path)
}

// Test builds only: move_any's rename answers EXDEV, so a folder takes the cross-device half on one filesystem.
#[cfg(test)]
thread_local! {
    static FORCE_EXDEV: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
pub(crate) fn test_force_exdev(force: bool) {
    FORCE_EXDEV.with(|f| f.set(force));
}

// Same filesystem is a rename; a different one is copy-then-remove, and the source only goes once the copy is complete.
pub fn move_any(src: &Path, dst: &Path, p: &mut Progress) -> Result<(), FleaError> {
    // A case twin reverses through the temp sibling, so redo shares the path the rename took.
    if crate::backend::renamecompat::is_same_item_for_move(src, dst) {
        crate::backend::renamecompat::case_twin_move(src, dst)?;
        if let Some(parent) = dst.parent() {
            touch(p, parent);
        }
        if let Some(parent) = src.parent() {
            touch(p, parent);
        }
        return Ok(());
    }
    #[cfg(test)]
    if FORCE_EXDEV.with(|f| f.get()) {
        return move_cross_device(src, dst, p);
    }
    match crate::backend::renamecompat::rename_noreplace(src, dst) {
        Ok(()) => {
            // One rename rewrote two directory entries, so both folders are confirmed.
            if let Some(parent) = dst.parent() {
                touch(p, parent);
            }
            if let Some(parent) = src.parent() {
                touch(p, parent);
            }
            Ok(())
        }
        Err(e) if e.raw_os_error() == Some(EXDEV) => move_cross_device(src, dst, p),
        // A FUSE mount that refuses RENAME_NOREPLACE (fuse.rclone, issue 282) moves like another device.
        Err(e) if crate::backend::renamecompat::needs_copy_fallback(src, &e) => move_cross_device(src, dst, p),
        Err(e) => Err(from_io("rename", &dst.to_string_lossy(), &e)),
    }
}

// The copy's folders are confirmed before the source goes, or a crash before the caller's flush loses the file.
pub(crate) fn move_cross_device(src: &Path, dst: &Path, p: &mut Progress) -> Result<(), FleaError> {
    copy_any(src, dst, p)?;
    // An unconfirmed copy is journaled like a partial one, so undo can take it back while the source stays whole.
    if let Err(e) = confirm_dest(p, dst) {
        p.partial = Some(dst.to_path_buf());
        return Err(e);
    }
    remove_any(src)?;
    if let Some(parent) = src.parent() {
        touch(p, parent);
    }
    Ok(())
}

// The folders copy_any touched; held files settle first so the source goes only after the bytes.
fn confirm_dest(p: &mut Progress, dst: &Path) -> Result<(), FleaError> {
    let failed = match p.durability.as_mut() {
        Some(durability) => durability.flush_dirs_for(dst).is_err(),
        None => dst
            .parent()
            .map(crate::backend::durable::fsync_dir)
            .unwrap_or(Ok(()))
            .is_err(),
    };
    if failed {
        return Err(unconfirmed(dst));
    }
    Ok(())
}

// One unconfirmed copy keeps its source, so a crash never loses the file.
fn unconfirmed(dst: &Path) -> FleaError {
    FleaError {
        where_: "move".to_string(),
        path: dst.to_string_lossy().to_string(),
        msg: crate::backend::durable::DIR_UNCONFIRMED.to_string(),
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
#[cfg(test)]
#[path = "copyfile_read_tests.rs"]
mod read_tests;
