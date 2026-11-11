// Issue 370: a stage on a copy-fallback mount uploads while the job runs and will not go after, so it lives on this computer.
use crate::backend::archivefree::format_size;
use crate::backend::archivework::{Work, WORK_PREFIX};
use crate::backend::fsinfo;
use crate::backend::mountinfo::mount_type_in;
use crate::backend::opsreq::op_err;
use crate::error::{from_io, FleaError};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

// The FUSE types renamecompat::needs_fuse_fallback_in copies onto; a test checks the two agree on every type it names.
const COPY_FALLBACK_MOUNTS: [&str; 2] = ["fuse.rclone", "fuse.megafs"];

#[cfg(test)]
thread_local! {
    static FORCE_OFF: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
    static ROOTS: std::cell::RefCell<Option<Vec<PathBuf>>> = const { std::cell::RefCell::new(None) };
    static LAST: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

// The folder the last work_for on this thread made its work folder in, so a test reads where a job staged.
#[cfg(test)]
pub(crate) fn test_take_last() -> Option<PathBuf> {
    LAST.with(|l| l.borrow_mut().take())
}

// A test treats everything under this folder as a copy-fallback mount, so no FUSE mount is needed.
#[cfg(test)]
pub(crate) fn test_force_off_mount(mount: Option<&Path>) {
    FORCE_OFF.with(|m| *m.borrow_mut() = mount.map(Path::to_path_buf));
}

// A test names the local roots in order, so nothing is written to the real cache.
#[cfg(test)]
pub(crate) fn test_local_roots(roots: Option<Vec<PathBuf>>) {
    ROOTS.with(|r| *r.borrow_mut() = roots);
}

// need is the stage's size in bytes, read only when the stage leaves the mount; what and leaf name the job in a refusal.
pub(crate) fn work_for(
    dest_parent: &Path,
    tag: &str,
    what: &str,
    leaf: &str,
    need: impl FnOnce() -> u64,
) -> Result<Work, FleaError> {
    let work = choose(dest_parent, tag, what, leaf, need);
    #[cfg(test)]
    if let Ok(work) = &work {
        LAST.with(|l| *l.borrow_mut() = work.dir.parent().map(Path::to_path_buf));
    }
    work
}

fn choose(
    dest_parent: &Path,
    tag: &str,
    what: &str,
    leaf: &str,
    need: impl FnOnce() -> u64,
) -> Result<Work, FleaError> {
    if !stages_off(dest_parent) {
        return Work::new(dest_parent, tag);
    }
    let need = need();
    let mut most = 0;
    let mut unusable = None;
    for root in local_roots() {
        // A home folder that is itself on such a mount is no better than the destination.
        if stages_off(&root) || make_private(&root).is_err() {
            continue;
        }
        let Some(info) = fsinfo::read(&root) else {
            continue;
        };
        most = most.max(info.free);
        if need > info.free {
            continue;
        }
        match Work::new(&root, tag).and_then(private) {
            Ok(work) => return Ok(work),
            Err(error) => unusable = Some(error),
        }
    }
    // A root with room that would not take the work folder says why, rather than a size it does not lack.
    if let Some(error) = unusable {
        return Err(error);
    }
    Err(op_err(
        what,
        leaf,
        &format!(
            "{} needs {} free on this computer before it goes to the drive, and the most free is {}",
            leaf,
            format_size(need),
            format_size(most)
        ),
    ))
}

// True when dest sits on a mount whose renames renamecompat answers with a copy; a symlinked path is read both ways.
pub(crate) fn stages_off(dest: &Path) -> bool {
    #[cfg(test)]
    if FORCE_OFF.with(|m| {
        m.borrow()
            .as_ref()
            .is_some_and(|mount| dest.starts_with(mount))
    }) {
        return true;
    }
    let body = std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
    // resolved_in stops at the first prefix inside a FUSE mount, so no syscall lands on the mount here.
    stages_off_in(dest, &body)
        || stages_off_in(&crate::backend::extclass::resolved_in(dest, &body), &body)
}

// Sample input: "/run/user/1000/gvfs/dav:host=slot,ssl=true/notes", or a path under a fuse.rclone line.
pub(crate) fn stages_off_in(dest: &Path, mountinfo: &str) -> bool {
    let text = dest.to_string_lossy();
    if text.starts_with("/run/user/") && text.contains("/gvfs/dav:") {
        return true;
    }
    mount_type_in(dest, mountinfo)
        .is_some_and(|fstype| COPY_FALLBACK_MOUNTS.contains(&fstype.as_str()))
}

// A stage this old whose Flea is gone was left by a kill; a live job's stage always has its Flea running.
pub(crate) const STALE_AFTER: Duration = Duration::from_secs(24 * 60 * 60);

fn cache_root() -> PathBuf {
    crate::userfile::cache_home().join("flea").join("work")
}

// Backend start: only the cache's own folder is swept, since /var/tmp and /tmp are shared and the system clears them.
pub fn sweep_at_start() -> Option<std::thread::JoinHandle<usize>> {
    let root = cache_root();
    // The common case is no folder at all, which costs one stat and no thread.
    if !root.is_dir() {
        return None;
    }
    std::thread::Builder::new()
        .name("flea-worksweep".into())
        .spawn(move || sweep(&root, SystemTime::now()))
        .ok()
}

// Removes every stage under root at least STALE_AFTER old whose process is gone, and answers how many went.
pub(crate) fn sweep(root: &Path, now: SystemTime) -> usize {
    let Ok(entries) = std::fs::read_dir(root) else {
        return 0;
    };
    let mut cleared = 0;
    for entry in entries.flatten() {
        let Some(pid) = stage_pid(&entry.file_name().to_string_lossy()) else {
            continue;
        };
        // symlink_metadata, so a link that happens to carry a stage's name is never followed out of the cache.
        let Ok(meta) = entry.path().symlink_metadata() else {
            continue;
        };
        let old = meta
            .modified()
            .ok()
            .and_then(|at| now.duration_since(at).ok())
            .is_some_and(|age| age >= STALE_AFTER);
        if meta.is_dir() && old && !running(pid) && std::fs::remove_dir_all(entry.path()).is_ok() {
            cleared += 1;
        }
    }
    cleared
}

// Sample input: ".flea-work-ext-4123-7" is pid 4123's seventh stage (Work::new); any other name is not a stage.
fn stage_pid(name: &str) -> Option<u32> {
    let mut parts = name.strip_prefix(WORK_PREFIX)?.rsplitn(3, '-');
    parts.next()?.parse::<usize>().ok()?;
    let pid = parts.next()?.parse().ok()?;
    parts.next().filter(|tag| !tag.is_empty())?;
    Some(pid)
}

// A pid reused by another program also reads as running, which only keeps a stage one more start.
fn running(pid: u32) -> bool {
    pid == std::process::id() || Path::new("/proc").join(pid.to_string()).exists()
}

// The cache first because it is on disk, then /var/tmp, then the temp folder, which is often memory.
fn local_roots() -> Vec<PathBuf> {
    #[cfg(test)]
    if let Some(roots) = ROOTS.with(|r| r.borrow().clone()) {
        return roots;
    }
    let mut roots = vec![cache_root(), PathBuf::from("/var/tmp")];
    let temp = std::env::temp_dir();
    if !roots.contains(&temp) {
        roots.push(temp);
    }
    roots
}

// /var/tmp and /tmp are shared, so the work folder in them shows nothing of the job to another user.
fn private(work: Work) -> Result<Work, FleaError> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&work.dir, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| from_io("archive", &work.dir.to_string_lossy(), &e))?;
    Ok(work)
}

// Only the cache's own folder is made, and it is the user's alone; /var/tmp and /tmp already exist.
fn make_private(root: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    if root.is_dir() {
        return Ok(());
    }
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(root)
}

// The bytes of every regular file under the named items, links not followed, so a compress knows its stage's size.
pub(crate) fn tree_bytes(parent: &Path, names: &[String]) -> u64 {
    let mut total: u64 = 0;
    let mut stack: Vec<PathBuf> = names.iter().map(|n| parent.join(n)).collect();
    while let Some(path) = stack.pop() {
        let Ok(meta) = path.symlink_metadata() else {
            continue;
        };
        if meta.is_file() {
            total = total.saturating_add(meta.len());
        } else if meta.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&path) {
                stack.extend(entries.flatten().map(|e| e.path()));
            }
        }
    }
    total
}

// The bytes of the named files, links followed as a convert reads them.
pub(crate) fn files_bytes(paths: &[PathBuf]) -> u64 {
    paths
        .iter()
        .filter_map(|p| p.metadata().ok())
        .fold(0u64, |sum, m| sum.saturating_add(m.len()))
}

#[cfg(test)]
#[path = "workhome_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "workhome_sweep_tests.rs"]
mod sweep_tests;
