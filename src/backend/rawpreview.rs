// Copies a camera RAW's largest embedded JPEG into the user cache once per file version, upright whatever the camera stored.
use crate::backend::imagesize;
use crate::backend::md5;
use crate::backend::rawembed::{inspect, JPEG_PREFIX};
use crate::backend::rawtiff::facts_from_slice;
use crate::backend::regfile::{open_if_regular, open_regular};
use crate::oflags::O_NOFOLLOW;
use crate::backend::thumbcache::uri_for;
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::fs::FileTimes;
use std::time::{Duration, SystemTime};

const FILE_MODE: u32 = 0o600;
const DIR_MODE: u32 = 0o700;
const EXIF_MAGIC: &[u8] = b"Exif\0\0";
// Marker, length 34, "Exif\0\0", "II" 42 and IFD0 at 8, one entry: Orientation (0x0112), SHORT, count 1, no next IFD.
const EXIF_ORIENT: [u8; 36] = [
    0xFF, 0xE1, 0x00, 0x22, b'E', b'x', b'i', b'f', 0, 0, b'I', b'I', 0x2A, 0, 8, 0, 0, 0, 1, 0, 0x12, 0x01, 3, 0, 1, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0,
];
// Where the orientation value sits in that segment.
const EXIF_ORIENT_AT: usize = 28;
// JPEG marker bytes: each marker is MARKER_START then one of these, and a length-prefixed segment follows all but SOS and EOI.
const MARKER_START: u8 = 0xFF;
const SOF_FIRST: u8 = 0xC0;
const SOF_LAST: u8 = 0xCF;
const DHT: u8 = 0xC4;
const JPG: u8 = 0xC8;
const DAC: u8 = 0xCC;
const EOI: u8 = 0xD9;
const SOS: u8 = 0xDA;
const APP1: u8 = 0xE1;
// The copies in the cache stay within this many files and bytes, least recently used first, and may sit over for a while: see evict.
const MAX_COPIES: usize = 4096;
const MIB: u64 = 1_048_576;
const MAX_BYTES: u64 = 512 * MIB;
// A temp this old belongs to a writer that died, since a live copy of a preview takes seconds; a copy made or served this recently is still being read.
const STALE_TEMP: Duration = Duration::from_secs(60);
// A scan lists and stats the whole directory, so one publish in this many pays for it; the first publish of a process is one.
const SCAN_EVERY: u32 = 32;
const TEMP_SUFFIX: &str = ".tmp";
const COPY_SUFFIX: &str = ".jpg";
// Two threads of this process can extract the same file at once, so each writes a temp of its own.
static NEXT_TEMP: AtomicU32 = AtomicU32::new(0);
// Publishes this process has made, which paces its eviction scans; the cache dir is shared, so each process scans on its own count.
static PUBLISHES: AtomicU32 = AtomicU32::new(0);

// What one extraction holds the cache to: the caps, the clock the ages are read against, and the counter that paces the scans.
struct Bound<'a> {
    copies: usize,
    bytes: u64,
    now: SystemTime,
    publishes: &'a AtomicU32,
}

// The copy and what a caller needs to draw it: stored pixels plus the turn a viewer applies, as imagesize reads them.
pub struct Extracted {
    pub file: PathBuf,
    pub width: u32,
    pub height: u32,
    pub orientation: u8,
}

pub fn cache_dir() -> PathBuf {
    crate::userfile::cache_home().join("flea").join("raw")
}

// Named by the RAW's URI, the key the thumbnail cache uses for the same file.
fn copy_path(dir: &Path, src: &Path) -> PathBuf {
    dir.join(format!("{}{COPY_SUFFIX}", md5::hex(uri_for(src).as_bytes())))
}

// None when the file is no RAW, holds no JPEG preview, or dir cannot be written; a fresh copy costs no walk and one atime write.
pub fn ensure_in(dir: &Path, src: &Path) -> Option<Extracted> {
    ensure_bounded(dir, src, &Bound { copies: MAX_COPIES, bytes: MAX_BYTES, now: SystemTime::now(), publishes: &PUBLISHES })
}

fn ensure_bounded(dir: &Path, src: &Path, bound: &Bound) -> Option<Extracted> {
    // Read before the bytes, so a RAW rewritten mid-copy leaves a stamp that no longer matches and extracts again.
    let stamp = std::fs::metadata(src).ok()?.modified().ok()?;
    let file = copy_path(dir, src);
    if fresh(&file, stamp) {
        mark_used(&file);
    } else {
        publish(dir, &file, src, stamp)?;
        if bound.publishes.fetch_add(1, Ordering::Relaxed) % SCAN_EVERY == 0 {
            evict(dir, bound);
        }
    }
    let h = imagesize::header(&file)?;
    Some(Extracted { file, width: h.width, height: h.height, orientation: h.orientation })
}

// The copy carries its RAW's mtime, and a symlink at the path is never a copy.
fn fresh(file: &Path, stamp: SystemTime) -> bool {
    std::fs::symlink_metadata(file)
        .map(|m| m.file_type().is_file() && m.len() > 0 && m.modified().ok() == Some(stamp))
        .unwrap_or(false)
}

// A cache hit sets the copy's atime to now, which moves its ctime too and leaves its mtime on the RAW's; a failure only ages the copy sooner.
fn mark_used(file: &Path) {
    if let Ok(f) = open_if_regular(file, O_NOFOLLOW) {
        let _ = f.set_times(FileTimes::new().set_accessed(SystemTime::now()));
    }
}

// When a copy was last made or served: its ctime, because its mtime is pinned to its RAW's and atime may be mounted lazy.
fn last_used(meta: &std::fs::Metadata) -> SystemTime {
    let secs = u64::try_from(meta.ctime()).unwrap_or(0);
    SystemTime::UNIX_EPOCH + Duration::new(secs, u32::try_from(meta.ctime_nsec()).unwrap_or(0))
}

// Stale temps go, then the least recently used copies down to the bound; a copy made or served within STALE_TEMP waits for the first scan after it ages, and each is re-read just before its unlink, so a serve in the one syscall after that re-read can still lose it.
fn evict(dir: &Path, bound: &Bound) {
    evict_between(dir, bound, || {});
}

// A copy gone before its turn counts as removed, while one served since the scan is spared.
fn removed_if_unused(path: &Path, seen: SystemTime) -> bool {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return true,
        Err(_) => return false,
    };
    if !meta.file_type().is_file() || last_used(&meta) > seen {
        return false;
    }
    match std::fs::remove_file(path) {
        Ok(()) => true,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => true,
        Err(_) => false,
    }
}

// The hook runs between the scan and the first removal, which is where a serve or a re-publish lands in a test.
fn evict_between(dir: &Path, bound: &Bound, between: impl FnOnce()) {
    let Ok(listing) = std::fs::read_dir(dir) else { return };
    let mut old: Vec<(SystemTime, PathBuf, u64)> = Vec::new();
    let (mut count, mut total) = (0usize, 0u64);
    for entry in listing.flatten() {
        let path = entry.path();
        let Ok(meta) = std::fs::symlink_metadata(&path) else { continue };
        if !meta.file_type().is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let stale = bound.now.duration_since(last_used(&meta)).is_ok_and(|age| age > STALE_TEMP);
        if name.ends_with(TEMP_SUFFIX) {
            if stale {
                let _ = std::fs::remove_file(&path);
            }
        } else if name.ends_with(COPY_SUFFIX) && crate::cachewrite::allowed(&path) {
            count += 1;
            total += meta.len();
            if stale {
                old.push((last_used(&meta), path, meta.len()));
            }
        }
    }
    old.sort();
    between();
    for (seen, path, size) in old {
        if count <= bound.copies && total <= bound.bytes {
            break;
        }
        if removed_if_unused(&path, seen) {
            count -= 1;
            total -= size;
        }
    }
}

// The orientation of the first Exif segment among the JPEG's leading segments; None when it names none.
// Sample input, the leading bytes of a camera's preview: FF D8 (SOI), FF E1 00 22 "Exif\0\0" 49 49 2A 00 08 00 00 00 ... (the Exif segment, length counting its own two bytes), FF DB ... (later segments), FF C0 ... (a frame header).
fn head_orientation(head: &[u8]) -> Option<u16> {
    let mut at = 2;
    while head.get(at)? == &MARKER_START {
        let marker = *head.get(at + 1)?;
        if marker == SOS || marker == EOI || ((SOF_FIRST..=SOF_LAST).contains(&marker) && ![DHT, JPG, DAC].contains(&marker)) {
            return None;
        }
        let len = u16::from_be_bytes([*head.get(at + 2)?, *head.get(at + 3)?]) as usize;
        let body = head.get(at + 4..at + 2 + len)?;
        if marker == APP1 && body.starts_with(EXIF_MAGIC) {
            return facts_from_slice(&body[EXIF_MAGIC.len()..], false).orientation;
        }
        at += 2 + len;
    }
    None
}

// Temp, exclusive create, rename: a planted path is never followed and a reader sees a whole copy or none.
fn publish(dir: &Path, file: &Path, src: &Path, stamp: SystemTime) -> Option<()> {
    crate::cachewrite::require(file).ok()?;
    let (preview, facts) = inspect(src)?;
    let preview = preview?;
    let mut input = open_regular(src)?;
    input.seek(SeekFrom::Start(preview.offset)).ok()?;
    let mut head = vec![0u8; preview.length.min(JPEG_PREFIX) as usize];
    input.read_exact(&mut head).ok()?;
    if !head.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    let turn = facts.orientation.filter(|o| (2..=8).contains(o)).filter(|_| head_orientation(&head).is_none());
    std::fs::DirBuilder::new().recursive(true).mode(DIR_MODE).create(dir).ok()?;
    let temp = file.with_extension(format!("{}.{}{TEMP_SUFFIX}", std::process::id(), NEXT_TEMP.fetch_add(1, Ordering::Relaxed)));
    let _ = std::fs::remove_file(&temp);
    let mut out = std::fs::OpenOptions::new().write(true).create_new(true).mode(FILE_MODE).open(&temp).ok()?;
    let rest = preview.length - head.len() as u64;
    let written = (|| -> std::io::Result<bool> {
        out.write_all(&head[..2])?;
        if let Some(o) = turn {
            let mut seg = EXIF_ORIENT;
            seg[EXIF_ORIENT_AT..EXIF_ORIENT_AT + 2].copy_from_slice(&o.to_le_bytes());
            out.write_all(&seg)?;
        }
        out.write_all(&head[2..])?;
        let copied = std::io::copy(&mut input.take(rest), &mut out)?;
        out.set_modified(stamp)?;
        Ok(copied == rest)
    })();
    if !matches!(written, Ok(true)) || std::fs::rename(&temp, file).is_err() {
        let _ = std::fs::remove_file(&temp);
        return None;
    }
    Some(())
}

#[cfg(test)]
#[path = "rawpreview_tests.rs"]
mod tests;
