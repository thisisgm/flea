// The hidden siblings of copies that died into a folder go before the next transfer lands there; see copystage.rs.
use super::copystage::{boot, PREFIX};
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// Kernel network filesystems another machine may be writing the same folder through: nfs, cifs, smb2, 9p, ceph.
const SHARED: [i64; 5] = [0x6969, 0xFF534D42, 0xFE534D42, 0x01021997, 0x00C36400];
// A sibling from another boot on a share may belong to a copy still running on another machine; one this old is not.
const FOREIGN_AGE: Duration = Duration::from_secs(60 * 60);
const ESRCH: i32 = 3;

extern "C" {
    fn geteuid() -> u32;
}

// Sample input: ".flea-part-3f2a9c1e-48211-7" gives ("3f2a9c1e", 48211); ".flea-part-notes.txt" gives None.
fn parse(leaf: &str) -> Option<(&str, i32)> {
    let mut parts = leaf.strip_prefix(PREFIX)?.split('-');
    let (boot, pid, seq) = (parts.next()?, parts.next()?, parts.next()?);
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let hex = boot.len() == 8 && boot.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if parts.next().is_some() || !hex || !digits(pid) || !digits(seq) {
        return None;
    }
    Some((boot, pid.parse().ok()?))
}

// The ctime, since keep_mtime backdates a live sibling's mtime; a folder's never moves for writes deeper in it, so it has none.
fn unchanged_for(meta: &std::fs::Metadata) -> Option<Duration> {
    if meta.is_dir() {
        return None;
    }
    let changed = UNIX_EPOCH + Duration::from_secs(u64::try_from(meta.ctime()).ok()?);
    SystemTime::now().duration_since(changed).ok()
}

// Stale once its copy cannot finish: its process left this boot, or another boot wrote it (on a share, unchanged for FOREIGN_AGE).
fn stale(boot_of: &str, pid: i32, unchanged: Option<Duration>, shared: bool) -> bool {
    if boot_of != boot() {
        return !shared || unchanged.is_some_and(|age| age >= FOREIGN_AGE);
    }
    if pid <= 1 || pid as u32 == std::process::id() {
        return false;
    }
    matches!(crate::procsignal::signal_process(pid, 0), Err(e) if e.raw_os_error() == Some(ESRCH))
}

// Only this user's files and folders whose whole name parses as a sibling are touched, so no name a person chose matches.
pub(crate) fn sweep(dir: &Path, cancel: &AtomicBool) -> usize {
    let magic = crate::backend::fsinfo::magic_of(dir);
    if !super::copystage::stages(magic) {
        return 0;
    }
    let shared = magic.is_some_and(|m| SHARED.contains(&m));
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    let me = unsafe { geteuid() };
    let mut removed = 0;
    for entry in entries.flatten() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let name = entry.file_name();
        let Some((boot_of, pid)) = name.to_str().and_then(parse) else { continue };
        let path = entry.path();
        let Ok(meta) = path.symlink_metadata() else { continue };
        if meta.uid() != me || !stale(boot_of, pid, unchanged_for(&meta), shared) {
            continue;
        }
        let gone = if meta.is_file() {
            std::fs::remove_file(&path).is_ok()
        } else if meta.is_dir() {
            super::copyfile::open_dir(&path).is_ok() && super::copyfile::remove_tree(&path).is_ok()
        } else {
            false
        };
        removed += usize::from(gone);
    }
    removed
}

// The same shape under a boot that is not this one.
#[cfg(test)]
pub(crate) fn test_foreign_leaf(pid: u32, seq: u64) -> String {
    let other = if boot() == "deadbeef" { "beefdead" } else { "deadbeef" };
    format!("{PREFIX}{other}-{pid}-{seq}")
}

// Whether another boot's sibling with this metadata is stale, on a share or a local drive.
#[cfg(test)]
pub(crate) fn test_stale_foreign(meta: &std::fs::Metadata, shared: bool) -> bool {
    test_stale_foreign_after(unchanged_for(meta), shared)
}

// How long a sibling counts as unchanged, None for a folder.
#[cfg(test)]
pub(crate) fn test_unchanged_for(meta: &std::fs::Metadata) -> Option<Duration> {
    unchanged_for(meta)
}

// The same judgement for a sibling unchanged this long, since a test cannot set a ctime back.
#[cfg(test)]
pub(crate) fn test_stale_foreign_after(unchanged: Option<Duration>, shared: bool) -> bool {
    let other = if boot() == "deadbeef" { "beefdead" } else { "deadbeef" };
    stale(other, 4242, unchanged, shared)
}
