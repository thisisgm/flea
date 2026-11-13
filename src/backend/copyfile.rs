// The copy primitives every transfer is built from: streaming, symlink-preserving, and refusing to overwrite.
use crate::error::{from_io, FleaError};
use std::io::{Seek, SeekFrom, Write};
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
// st_blocks counts 512-byte units whatever the filesystem block size.
const BLOCK_BYTES: u64 = 512;
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
    // A file with fewer blocks than its length has holes, which the copy seeks over; a FUSE daemon, or a drive statfs cannot name, may refuse a write past its end, so it takes zeros.
    let holes_kept = dest_magic.is_some_and(|m| m != super::copyread::FUSE_MAGIC);
    let sparse = holes_kept && std::os::unix::fs::MetadataExt::blocks(&src_meta).saturating_mul(BLOCK_BYTES) < src_meta.len();
    // The end of the data extent the copy is in; a dense file is one extent and never asks.
    let mut extent_end = if sparse { 0 } else { u64::MAX };
    loop {
        if cancelled(p) {
            // The partial file goes with the cancel: a half-written destination is not a result anyone asked for.
            drop(w);
            let _ = stage.discard();
            return Err(cancel_err(dst.named));
        }
        if done >= extent_end {
            match r.extent(done) {
                Ok(Some((start, end))) => {
                    if start > done {
                        if let Err(e) = w.seek(SeekFrom::Start(start)) {
                            return Err(left_half_written(p, dst, &stage, &w, from_io("copy", &dst.named.to_string_lossy(), &e)));
                        }
                        done = start;
                    }
                    extent_end = end;
                }
                Ok(None) => break,
                Err(e) => return Err(left_half_written(p, dst, &stage, &w, from_io("copy", &src.named.to_string_lossy(), &e))),
            }
        }
        let read = r.read_upto(p.cancel, extent_end - done);
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
    // A hole at the end has no data to write, so the copy takes the source's length instead.
    if sparse && done < src_meta.len() {
        if let Err(e) = w.set_len(src_meta.len()) {
            return Err(left_half_written(p, dst, &stage, &w, from_io("copy", &dst.named.to_string_lossy(), &e)));
        }
        done = src_meta.len();
        if !durable {
            report_landed(p, dst, done, total);
        }
    }
    if let Err(e) = w.flush() {
        return Err(left_half_written(p, dst, &stage, &w, from_io("copy", &dst.named.to_string_lossy(), &e)));
    }
    // A source another program cut short or rewrote while it was read gives a torn copy, so the copy goes and the item fails.
    if r.changed(&src_meta, src.at) {
        return Err(torn(p, dst, &stage, w));
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
            // A held file closes in a wave after its batch's syncfs, so only its fresh size is checked here.
            if let Err(e) = landed::await_landed(dst.at, src_meta.len(), p.cancel) {
                if let Some(durability) = p.durability.as_mut() {
                    durability.note_file_failed();
                }
                record_open(p, dst.named, &w);
                return Err(left_partial(p, dst.named, e.error("copy", dst.named)));
            }
            // The bytes count when written, at most one batch ahead of the drive's own syncfs.
            report_landed(p, dst, done, total);
            if let Some(carried) = p.tree.as_mut() {
                *carried += done;
            }
            record_open(p, dst.named, &w);
            if let Some(durability) = p.durability.as_mut() {
                durability.hold(w, if inflight.is_some() { 0 } else { done });
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
    record_open(p, dst.named, &w);
    land(p, src_meta.len(), dst, w)?;
    if let Some(carried) = p.tree.as_mut() {
        *carried += done;
    }
    Ok(())
}

// gvfs answers fsync and close before a WebDAV upload ends, so the file counts once close succeeds and a fresh stat shows all of it.
fn land(p: &mut Progress, len: u64, dst: At, w: std::fs::File) -> Result<(), FleaError> {
    let landed = landed::close_file(w).map_err(landed::Unlanded::Failed).and_then(|()| landed::await_landed(dst.at, len, p.cancel));
    landed.map_err(|e| left_partial(p, dst.named, e.error("copy", dst.named)))
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
        return Some(format!("{name} needs {}, and the drive has {} free.", format_size(size), format_size(free)));
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
    static READ: std::sync::LazyLock<u32> = std::sync::LazyLock::new(|| {
        let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
        for line in status.lines() {
            if let Some(value) = line.strip_prefix("Umask:") {
                if let Ok(bits) = u32::from_str_radix(value.trim(), 8) {
                    return bits & 0o777;
                }
            }
        }
        0o022
    });
    *READ
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
fn drop_folder(p: &mut Progress, dst: At, folder: &Folder, e: FleaError) -> FleaError {
    let leaf = folder.at().file_name().map(|leaf| dst.named.with_file_name(leaf));
    #[cfg(test)]
    let removed = if STUCK_FOLDER.with(|v| v.replace(false)) { Err(()) } else { remove_tree(folder.at()) };
    #[cfg(not(test))]
    let removed = remove_tree(folder.at());
    p.partial = removed.err().and(leaf);
    // This folder's own manifest names paths that never existed, so undo judges the sibling whole; a merge keeps its records.
    if p.partial.is_some() && p.manifest.as_ref().is_some_and(|m| m.root() == dst.named) { p.manifest = None; }
    if let Some(durability) = p.durability.as_mut() {
        durability.forget_tree(dst.named);
    }
    e
}

// Inside a tree the torn file is named and skipped like a refusal; at the top it is the item's own failure.
fn torn(p: &mut Progress, dst: At, stage: &Stage, w: std::fs::File) -> FleaError {
    let e = super::unheld::marked(dst.named, if p.for_move { "changed during the move" } else { "changed during the copy" }.to_string());
    if stage.discard().is_ok() {
        return e;
    }
    left_half_written(p, dst, stage, &w, e)
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
    // A move whose source its folder will not give up fails before the first byte, with the source whole.
    if p.for_move {
        movable(src)?;
    }
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
        return tree::copy_dir_at(src, dst, p, inside);
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

// A 0500 copy refuses removal, so a folder this run made takes the owner's bits where its mode refuses; see deeptree.rs.
pub(super) fn remove_tree(at: &Path) -> Result<(), ()> {
    deep::remove_deep(at, true).map_err(|_| ())
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
    if let Err(e) = finish_move(src, dst, p.durability.as_deref_mut()) {
        if e.copy_to_undo {
            p.partial = Some(dst.to_path_buf());
        }
        return Err(e.error);
    }
    if let Some(parent) = src.parent() {
        touch(p, parent);
    }
    Ok(())
}

// Where a move's removal of its source stopped: before anything went, or partway through a folder.
pub(crate) enum Unremoved {
    Whole(FleaError),
    Part(FleaError),
}

// A move whose source would not go; copy_to_undo marks a whole source whose copy would not go back either.
pub(crate) struct Unmoved {
    pub error: FleaError,
    pub copy_to_undo: bool,
}

// A source wholly kept takes its copy back; one removed partway leaves the copy as the only whole one.
pub(crate) fn finish_move(src: &Path, dst: &Path, durability: Option<&mut super::durable::Durability>) -> Result<(), Unmoved> {
    // The source goes only while the destination shows the same kind and, for a file, every byte.
    if let Err(e) = landed::same_as_source(src, dst) {
        return Err(Unmoved { error: e.error("move", dst), copy_to_undo: true });
    }
    match remove_moved(src) {
        Ok(()) => Ok(()),
        Err(Unremoved::Part(error)) => Err(Unmoved { error, copy_to_undo: false }),
        Err(Unremoved::Whole(error)) => {
            let copy_to_undo = deep::remove_deep(dst, true).is_err();
            if let (false, Some(durability)) = (copy_to_undo, durability) {
                durability.forget_tree(dst);
            }
            Err(Unmoved { error, copy_to_undo })
        }
    }
}

// The folder must let this process take the item out; a sticky folder only lets the item's owner or its own.
pub(crate) fn movable(src: &Path) -> Result<(), FleaError> {
    let meta = src.symlink_metadata().map_err(|e| from_io("move", &src.to_string_lossy(), &e))?;
    match src.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        Some(parent) => deep::removable_from(parent, &meta).map_err(|e| from_io("move", &src.to_string_lossy(), &e)),
        None => Ok(()),
    }
}

// Whole only when the source is still there and nothing of it went; one already gone leaves its copy the whole one.
pub(crate) fn remove_moved(src: &Path) -> Result<(), Unremoved> {
    let mut gone = 0;
    let removed = movable(src).and_then(|()| {
        let meta = src.symlink_metadata().map_err(|e| from_io("move", &src.to_string_lossy(), &e))?;
        let r = if meta.is_dir() { deep::remove_counting(src, false, &mut gone) } else { std::fs::remove_file(src) };
        r.map_err(|e| from_io("move", &src.to_string_lossy(), &e))
    });
    removed.map_err(|e| match (gone, src.symlink_metadata().is_ok()) {
        (0, true) => Unremoved::Whole(e),
        (0, false) => Unremoved::Part(e),
        _ => Unremoved::Part(FleaError { msg: format!("{}; part of it was removed, and the copy holds all of it", e.msg), ..e }),
    })
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

fn cancel_err(path: &Path) -> FleaError {
    FleaError {
        where_: "copy".to_string(),
        path: path.to_string_lossy().to_string(),
        msg: "cancelled".to_string(),
    }
}

#[path = "copytree.rs"]
mod tree;
#[path = "landed.rs"]
pub(crate) mod landed;
#[path = "deeptree.rs"]
pub(crate) mod deep;

#[cfg(test)]
#[path = "copyfile_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "copyfile_read_tests.rs"]
mod read_tests;
#[cfg(test)]
#[path = "copydeep_tests.rs"]
mod deep_tests;
#[cfg(test)]
#[path = "copymove_tests.rs"]
mod move_tests;
#[cfg(test)]
#[path = "landed_tests.rs"]
mod landed_tests;
#[cfg(test)]
#[path = "copywire_tests.rs"]
mod wire_tests;
