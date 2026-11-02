// The gvfs early listing adopts the launcher's gio output instead of spawning gio itself.
use crate::backend::gvfslist;
use crate::backend::listing::Listing;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// Launcher hands qs the prefetch file, path and launch start; empty is absent throughout.
pub const PREFETCH_ENV: &str = "FLEA_GVFS_PREFETCH";
pub const PATH_ENV: &str = "FLEA_GVFS_PATH";
pub const START_ENV: &str = "FLEA_GVFS_START";
// A published file older than this is refused, even one written after the launch.
const STALE_SECS: u64 = 10;
// Leftovers stop being adoptable long before this reaps them.
const SWEEP_SECS: u64 = 60;
// No dest and no claim this long after the launch means the child is gone.
const GRACE_SECS: u64 = 2;
// Wait between dest polls while the prefetch child is still enumerating.
const POLL_MS: u64 = 20;
// A 10k NAS listing is about a megabyte; anything past this is not gio's output.
const MAX_BYTES: u64 = 64 * 1024 * 1024;
// Sample gio failure outcome: "flea-gvfs-fail\n", never a valid listing line.
const FAIL_MARKER: &[u8] = b"flea-gvfs-fail\n";
// A prefetch is adopted once; this stops a second listing adopting a late arrival.
static CONSUMED: AtomicBool = AtomicBool::new(false);
// Two prepares in one process still name different files.
static NEXT: AtomicU64 = AtomicU64::new(0);

// std already links libc, so this symbol is declared here rather than taking a crate.
extern "C" {
    fn geteuid() -> u32;
}

// The one adoption the backend attempts: None unless the launcher named all three.
pub struct Prefetch {
    pub dest: PathBuf,
    pub path: String,
    pub start_ms: u64,
}

// Sample input: FLEA_GVFS_PATH "/run/user/1000/gvfs/smb-share:server=x,share=y/dir".
pub fn env_prefetch() -> Option<Prefetch> {
    env_prefetch_from(|name| std::env::var_os(name))
}

// Split so a test names the three values without writing this process's environment.
pub(crate) fn env_prefetch_from(get: impl Fn(&str) -> Option<std::ffi::OsString>) -> Option<Prefetch> {
    let dest = get(PREFETCH_ENV).filter(|v| !v.is_empty()).map(PathBuf::from)?;
    let path = get(PATH_ENV).and_then(|v| v.into_string().ok()).filter(|v| !v.is_empty())?;
    let start_ms = get(START_ENV)?.into_string().ok()?.parse::<u64>().ok()?;
    Some(Prefetch { dest, path, start_ms })
}

// $XDG_RUNTIME_DIR/flea, or None when the session names none.
pub fn runtime_dir() -> Option<PathBuf> {
    runtime_dir_from(std::env::var_os("XDG_RUNTIME_DIR"))
}

// Split so a test names the session's runtime root without writing this process's environment.
pub(crate) fn runtime_dir_from(root: Option<std::ffi::OsString>) -> Option<PathBuf> {
    let root = root.filter(|v| !v.is_empty())?;
    Some(PathBuf::from(root).join("flea"))
}

// A directory this user owns and only this user can enter; anything else refuses.
fn check_dir(dir: &Path) -> Result<(), String> {
    let meta = std::fs::symlink_metadata(dir)
        .map_err(|e| format!("gvfs prefetch dir {} could not be read ({:?})", dir.display(), e.kind()))?;
    if meta.file_type().is_symlink() {
        return Err(format!("gvfs prefetch dir {} is a symlink", dir.display()));
    }
    if !meta.is_dir() {
        return Err(format!("gvfs prefetch dir {} is not a directory", dir.display()));
    }
    if meta.mode() & 0o777 != 0o700 {
        return Err(format!("gvfs prefetch dir {} is not 0700", dir.display()));
    }
    if meta.uid() != unsafe { geteuid() } {
        return Err(format!("gvfs prefetch dir {} is not owned by this user", dir.display()));
    }
    Ok(())
}

// Our own runtime dir, created when missing and refused when wrong; never repaired.
fn ensure_dir(dir: &Path) -> Result<(), String> {
    if std::fs::symlink_metadata(dir).is_err() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("gvfs prefetch dir {} could not be created ({:?})", dir.display(), e.kind()))?;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| format!("gvfs prefetch dir {} could not be made 0700 ({:?})", dir.display(), e.kind()))?;
    }
    check_dir(dir)
}

// A previous launch's leftovers, reaped by age; best effort, so errors are ignored.
fn sweep_dir(dir: &Path, now: SystemTime) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        if !entry.file_name().to_string_lossy().starts_with("gvfs-") {
            continue;
        }
        let path = entry.path();
        if !crate::persistdoc::replaceable(&path, MAX_BYTES, b"", recognized) { continue; }
        let keep = std::fs::symlink_metadata(&path)
            .ok()
            .filter(|meta| !meta.file_type().is_symlink() && meta.is_file())
            .and_then(|meta| meta.modified().ok())
            .and_then(|mtime| now.duration_since(mtime).ok())
            .is_some_and(|age| age <= Duration::from_secs(SWEEP_SECS));
        if !keep {
            let _ = std::fs::remove_file(&path);
        }
    }
}

// The launcher's side: our dir, last leftovers gone, this launch's file named.
pub fn prepare(path: &str) -> Option<(PathBuf, u64)> {
    prepare_for(path, runtime_dir())
}

// Split so a test names the runtime dir without writing this process's environment.
pub(crate) fn prepare_for(path: &str, runtime: Option<PathBuf>) -> Option<(PathBuf, u64)> {
    if !gvfslist::is_gvfs(Path::new(path)) {
        return None;
    }
    prepare_in(runtime)
}

// Split so a test names the runtime dir without writing this process's environment.
pub(crate) fn prepare_in(dir: Option<PathBuf>) -> Option<(PathBuf, u64)> {
    let dir = dir?;
    // A refused runtime dir says why on stderr, then the launch lists the share itself.
    if let Err(e) = ensure_dir(&dir) {
        eprintln!("flea: {}, so the share lists without a head start", e);
        return None;
    }
    let now = SystemTime::now();
    sweep_dir(&dir, now);
    let start_ms = now.duration_since(UNIX_EPOCH).ok()?.as_millis() as u64;
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dest = dir.join(format!("gvfs-{}-{}-{}.list", std::process::id(), start_ms, n));
    Some((dest, start_ms))
}

// The gio half the combined launch helper shares: the same call the backend would make, hidden included.
pub(crate) fn run_in(path: &str, dest: &Path, gio: &str, runtime: Option<&Path>) -> i32 {
    let Some(dir) = dest.parent() else {
        eprintln!("flea: gvfs prefetch destination has no parent directory");
        return 2;
    };
    // dest must be a fresh name inside our own runtime dir, never an arbitrary path.
    if Some(dir) != runtime {
        eprintln!("flea: gvfs prefetch destination {} is not inside the runtime dir", dest.display());
        return 2;
    }
    if !gvfslist::is_gvfs(Path::new(path)) {
        eprintln!("flea: gvfs prefetch needs a gvfs path, got {}", path);
        return 2;
    }
    if let Err(e) = check_dir(dir) {
        eprintln!("flea: {}", e);
        return 2;
    }
    match gvfslist::raw_output(path, gio, gvfslist::GIO_TIMEOUT) {
        Ok(bytes) => match publish(dest, &bytes) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("flea: {}", e);
                1
            }
        },
        Err(e) => {
            // A failed gio still publishes an outcome, so the backend stops waiting at once.
            let _ = publish(dest, FAIL_MARKER);
            eprintln!("flea: gvfs prefetch for {} failed ({}), the window lists it itself", path, e);
            1
        }
    }
}

// Our own temp file, created exclusively at 0600, then a rename; readers never see a partial.
fn publish(dest: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = PathBuf::from(format!("{}.{}.tmp", dest.display(), std::process::id()));
    let _ = std::fs::remove_file(&tmp);
    let written = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)
        .map_err(|e| format!("gvfs prefetch temp {} could not be created ({:?})", tmp.display(), e.kind()))
        .and_then(|mut file| {
            use std::io::Write;
            file.write_all(bytes)
                .map_err(|e| format!("gvfs prefetch temp {} could not be written ({:?})", tmp.display(), e.kind()))
        })
        .and_then(|()| {
            std::fs::hard_link(&tmp, dest).and_then(|()| std::fs::remove_file(&tmp))
                .map_err(|e| format!("gvfs prefetch {} could not be published ({:?})", dest.display(), e.kind()))
        });
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written
}

// The claim a reader renames dest to; a second reader finding it returns None at once.
pub(crate) fn claimed_path(dest: &Path) -> PathBuf {
    PathBuf::from(format!("{}.claimed", dest.display()))
}

// Best effort: only a regular file is ever truncated, so a fifo or device never blocks the scan.
fn shrink_claim(claimed: &Path) {
    if !std::fs::symlink_metadata(claimed).map(|m| m.is_file()).unwrap_or(false) {
        return;
    }
    let flags = crate::oflags::O_NOFOLLOW | crate::oflags::O_NONBLOCK;
    if let Ok(file) = std::fs::OpenOptions::new().write(true).custom_flags(flags).open(claimed) {
        if file.metadata().map(|m| m.is_file()).unwrap_or(false) {
            let _ = file.set_len(0);
        }
    }
}

// The backend's first gvfs scan of the prefetched path adopts the file, in the runtime dir the caller names; else None.
pub(crate) fn adopt_matching_in(path: &str, hidden: bool, prefetch: &Prefetch, runtime: Option<&Path>) -> Option<(Listing, f64)> {
    if prefetch.path != path {
        return None;
    }
    if CONSUMED.swap(true, Ordering::SeqCst) {
        return None;
    }
    // corner: a listing still streaming 15 s after launch is re-listed by the scan; 10k NAS rows take about 1.1 s.
    let deadline = UNIX_EPOCH + Duration::from_millis(prefetch.start_ms) + gvfslist::GIO_TIMEOUT;
    adopt_in(path, hidden, &prefetch.dest, deadline, prefetch.start_ms, runtime)
}

// Split so a test can name the launch start and runtime dir without touching the environment.
pub(crate) fn adopt_in(
    path: &str,
    hidden: bool,
    dest: &Path,
    deadline: SystemTime,
    start_ms: u64,
    runtime: Option<&Path>,
) -> Option<(Listing, f64)> {
    let t = Instant::now();
    // Only a file in our own runtime dir is this launch's; anything else is left exactly where it is.
    let dir = dest.parent()?;
    if Some(dir) != runtime || check_dir(dir).is_err() {
        return None;
    }
    let launch = UNIX_EPOCH + Duration::from_millis(start_ms);
    let claimed = claimed_path(dest);
    // A claimed sibling means another backend already took this launch's file.
    if std::fs::symlink_metadata(&claimed).is_ok() {
        return None;
    }
    // No dest and no claim long after the launch means the child is gone; do not wait out the deadline.
    if std::fs::symlink_metadata(dest).is_err()
        && SystemTime::now().duration_since(launch).unwrap_or(Duration::ZERO) > Duration::from_secs(GRACE_SECS)
    {
        return None;
    }
    loop {
        if std::fs::symlink_metadata(dest).is_ok() {
            break;
        }
        // A claim landing mid-wait means the other reader won the rename below.
        if std::fs::symlink_metadata(&claimed).is_ok() {
            return None;
        }
        if SystemTime::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(POLL_MS));
    }
    // Refused bytes stay at their original path, without a claim or truncation.
    let bytes = read_prefetch(dest, start_ms)?;
    let adopted = if bytes == FAIL_MARKER { None } else {
        let text = std::str::from_utf8(&bytes).ok()?;
        Some((gvfslist::build_listing(text, hidden, path).ok()?, t.elapsed().as_secs_f64() * 1000.0))
    };
    // Atomic claim: only the rename winner adopts; the loser finds the claim above.
    if std::fs::rename(dest, &claimed).is_err() {
        return None;
    }
    shrink_claim(&claimed);
    adopted
}

// A listing cut short keeps whole rows up to its last newline, and a cut marker is a prefix of the marker.
fn recognized(bytes: &[u8]) -> bool {
    let Some(end) = bytes.iter().rposition(|b| *b == b'\n') else { return FAIL_MARKER.starts_with(bytes) };
    bytes == FAIL_MARKER || std::str::from_utf8(&bytes[..=end]).is_ok_and(|text| text.lines().all(|line| gvfslist::parse_line(line, true).is_ok()))
}

// The checks a reader applies before trusting a file it did not write itself; adopt_in has checked its dir.
fn read_prefetch(dest: &Path, start_ms: u64) -> Option<Vec<u8>> {
    let meta = std::fs::symlink_metadata(dest).ok()?;
    // A planted symlink is refused rather than followed; the open below repeats the refusal.
    if meta.file_type().is_symlink() || !meta.is_file() {
        return None;
    }
    if meta.uid() != unsafe { geteuid() } {
        return None;
    }
    let mtime = meta.modified().ok()?;
    let launch = UNIX_EPOCH + Duration::from_millis(start_ms);
    // Older than the launch is a previous launch's leftover, swept but not yet gone.
    if mtime < launch {
        return None;
    }
    // Freshness against a clock read after the wait; a future mtime reads as age zero.
    let age = SystemTime::now().duration_since(mtime).unwrap_or(Duration::ZERO);
    if age > Duration::from_secs(STALE_SECS) {
        return None;
    }
    if meta.len() > MAX_BYTES {
        return None;
    }
    let file = std::fs::OpenOptions::new().read(true).custom_flags(crate::oflags::O_NOFOLLOW).open(dest).ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let mut bytes = Vec::new();
    use std::io::Read;
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > MAX_BYTES {
        return None;
    }
    Some(bytes)
}

#[cfg(test)]
#[path = "gvfsprefetch_tests.rs"]
mod tests;
