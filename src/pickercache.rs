// Where the picker keeps a file it downloaded for a portal answer, and the sweep that ages it out.
use crate::paths::percent_decode;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// Downloads live here, outside the reply dir tools/flea-portal
// removes when it answers: the application reads the file after the dialog is gone.
const PICKER_CACHE_DIR: &str = "flea/picker";
const HOME_CACHE_DIR: &str = ".cache";
// An entry older than this at --pick start is a download no application still reads.
const PICKER_CACHE_KEEP: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const FETCH_FALLBACK_LEAF: &str = "download";

// Two fetches in one process must not share a dir, and the crate takes no dependency to name one.
static FETCH_SEQ: AtomicU64 = AtomicU64::new(0);

pub fn picker_cache_dir() -> PathBuf {
    cache_root(std::env::var_os("XDG_CACHE_HOME"), std::env::var_os("HOME")).join(PICKER_CACHE_DIR)
}

// The XDG rule thumbcache::default_root follows: an empty XDG_CACHE_HOME is unset, HOME/.cache is next.
fn cache_root(xdg: Option<OsString>, home: Option<OsString>) -> PathBuf {
    xdg.map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .or_else(|| home.map(|h| PathBuf::from(h).join(HOME_CACHE_DIR)))
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

// A fresh dir under the picker cache, joined with the URL's leaf: one dir per fetch, so two downloads
// of the same name never collide. The fetch request (backend/fetchreq.rs) downloads into it.
pub fn fetch_dest(leaf: &str) -> io::Result<PathBuf> {
    fetch_dest_in(&picker_cache_dir(), leaf)
}

fn fetch_dest_in(root: &Path, leaf: &str) -> io::Result<PathBuf> {
    std::fs::create_dir_all(root)?;
    // A clash is a same-tick repeat of the mix, so a few more draws are enough; an error past that is real.
    let mut last = None;
    for _ in 0..8 {
        let dir = root.join(random_name());
        match std::fs::create_dir(&dir) {
            Ok(()) => return Ok(dir.join(safe_leaf(leaf))),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => last = Some(e),
            Err(e) => return Err(e),
        }
    }
    Err(last.unwrap_or_else(|| io::Error::other("no fresh fetch dir")))
}

// Eight hex chars mixed from the pid, the clock and a counter; unique enough on one machine, not secret.
fn random_name() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let seq = FETCH_SEQ.fetch_add(1, Ordering::Relaxed);
    let mut x = nanos ^ (u64::from(std::process::id()) << 32) ^ seq.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    // splitmix64's finaliser, so a counter step changes every output bit.
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    format!("{:08x}", x as u32)
}

// The leaf is one file name inside its own dir, so anything that could leave that dir is replaced.
fn safe_leaf(leaf: &str) -> String {
    let name = percent_decode(leaf);
    if name.is_empty() || name == "." || name == ".." || name.contains('/') || name.contains('\0') {
        return FETCH_FALLBACK_LEAF.to_string();
    }
    name
}

// Runs before --pick opens its window; errors are ignored because a missing or busy cache is no reason
// to refuse a chooser.
pub fn sweep_picker_cache() {
    let cutoff = SystemTime::now().checked_sub(PICKER_CACHE_KEEP).unwrap_or(UNIX_EPOCH);
    sweep_older_than(&picker_cache_dir(), cutoff);
}

// Removes every subdir whose mtime is before the cutoff. Only dirs: a stray file is not this module's.
fn sweep_older_than(root: &Path, cutoff: SystemTime) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        // corner: DirEntry::metadata is lstat, so a symlink is not a dir here and its target is never removed.
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if !meta.is_dir() {
            continue;
        }
        if meta.modified().is_ok_and(|mtime| mtime < cutoff) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;
    #[test]
    fn the_picker_cache_follows_xdg_cache_home_then_home() {
        let d = |s: &str| Some(OsString::from(s));
        assert_eq!(cache_root(None, d("/home/u")), PathBuf::from("/home/u/.cache"));
        assert_eq!(cache_root(d(""), d("/home/u")), PathBuf::from("/home/u/.cache"));
        assert_eq!(cache_root(d("/var/cache/u"), d("/home/u")), PathBuf::from("/var/cache/u"));
        assert_eq!(cache_root(None, None), PathBuf::from("/tmp"));
        assert!(picker_cache_dir().ends_with("flea/picker"));
    }

    #[test]
    fn a_fetch_dest_is_a_fresh_dir_under_the_root_with_the_leaf() {
        let t = TestDir::new("fetchdest");
        let root = t.join("flea/picker");
        let a = fetch_dest_in(&root, "report.pdf").expect("dest");
        let b = fetch_dest_in(&root, "report.pdf").expect("dest");
        assert_eq!(a.file_name().unwrap(), "report.pdf");
        assert_ne!(a, b);
        assert!(a.parent().unwrap().is_dir());
        assert_eq!(a.parent().unwrap().parent().unwrap(), root);
        assert_eq!(a.parent().unwrap().file_name().unwrap().len(), 8);
        assert!(!a.exists());
    }

    #[test]
    fn a_leaf_that_could_leave_its_dir_becomes_download() {
        assert_eq!(safe_leaf(""), "download");
        assert_eq!(safe_leaf("."), "download");
        assert_eq!(safe_leaf(".."), "download");
        assert_eq!(safe_leaf("a/b"), "download");
        assert_eq!(safe_leaf("a%2Fb"), "download");
        assert_eq!(safe_leaf("a%00b"), "download");
        assert_eq!(safe_leaf("a%20b"), "a b");
        assert_eq!(safe_leaf("caf%C3%A9.txt"), "café.txt");
    }

    #[test]
    fn the_sweep_keeps_a_fresh_entry_and_drops_an_old_one() {
        let t = TestDir::new("sweep");
        let root = t.dir("flea/picker");
        let now = SystemTime::now();
        let day = Duration::from_secs(24 * 60 * 60);
        let fresh = t.dir("flea/picker/aaaaaaaa");
        let old = t.dir("flea/picker/bbbbbbbb");
        t.file("flea/picker/bbbbbbbb/download", "body");
        let stray = t.file("flea/picker/stray", "not a dir");
        let target = t.dir("elsewhere");
        let link = t.join("flea/picker/cccccccc");
        std::os::unix::fs::symlink(&target, &link).expect("symlink");
        set_mtime(&fresh, now - day);
        set_mtime(&old, now - 8 * day);
        set_mtime(&stray, now - 8 * day);
        set_mtime(&target, now - 8 * day);
        sweep_older_than(&root, now - 7 * day);
        assert!(fresh.is_dir());
        assert!(!old.exists());
        assert!(stray.is_file());
        assert!(link.symlink_metadata().is_ok());
        assert!(target.is_dir());
        // A missing root is the first run on a box, not an error.
        sweep_older_than(&t.join("absent"), now);
    }

    fn set_mtime(path: &Path, at: SystemTime) {
        let f = std::fs::File::open(path).expect("open for mtime");
        f.set_times(std::fs::FileTimes::new().set_modified(at)).expect("set mtime");
    }
}
