// Flea deletes its own dead Quickshell registry entries, which Quickshell keeps forever.
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

// Dead entries kept per shell id, so `qs log` still reaches the last crash.
pub const KEEP_DEAD: usize = 3;

// fcntl(2) lock query: F_GETLK answers F_UNLCK when no process holds the write lock.
const F_GETLK: i32 = 5;
const F_WRLCK: i32 = 1;
const F_UNLCK: i32 = 2;

// corner: Linux flock layout
#[repr(C)]
struct Flock {
    l_type: i16,
    l_whence: i16,
    l_start: i64,
    l_len: i64,
    l_pid: i32,
}

// std already links libc, so fcntl is declared here with the same variadic tail as the other three; lock call sites pass a *mut Flock.
extern "C" {
    fn fcntl(fd: i32, cmd: i32, ...) -> i32;
}

// The base is a parameter so tests prune a sandbox, never the real runtime dir.
pub fn prune_dead_in(base: &Path, shell_ids: &[&str]) {
    if !base.is_dir() {
        return;
    }
    // One readdir of by-pid per launch, readlink only.
    let by_pid = read_link_dir(&base.join("by-pid"));
    let uid = crate::backend::manifestdir::current_uid();
    for id in shell_ids {
        prune_shell(base, id, &by_pid, uid);
    }
}

// Every (link path, raw target) under a directory; a directory that cannot be read answers empty.
fn read_link_dir(dir: &Path) -> Vec<(PathBuf, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    entries
        .flatten()
        .filter_map(|entry| std::fs::read_link(entry.path()).ok().map(|target| (entry.path(), target)))
        .collect()
}

// One dead entry: its verified by-id dir, its by-shell link, and its log mtime for the keep-newest cut.
struct Dead {
    by_id: PathBuf,
    link: PathBuf,
    log_mtime: std::time::SystemTime,
}

// A lock query that fails is read as alive, so a failure keeps the entry rather than deleting it.
fn lock_dead(lock_path: &Path) -> bool {
    use std::os::fd::AsRawFd;
    let Ok(file) = std::fs::File::open(lock_path) else { return false };
    // An empty or missing lock is an instance still starting, which is never dead.
    if file.metadata().is_ok_and(|meta| meta.len() == 0) {
        return false;
    }
    let mut lock = Flock { l_type: F_WRLCK as i16, l_whence: 0, l_start: 0, l_len: 0, l_pid: 0 };
    if unsafe { fcntl(file.as_raw_fd(), F_GETLK, &mut lock as *mut Flock) } != 0 {
        return false;
    }
    lock.l_type == F_UNLCK as i16
}

fn prune_shell(base: &Path, id: &str, by_pid: &[(PathBuf, PathBuf)], uid: u32) {
    let shell_dir = base.join("by-shell").join(id);
    let Ok(entries) = std::fs::read_dir(&shell_dir) else { return };
    let mut dead = Vec::new();
    for entry in entries.flatten() {
        let link = entry.path();
        let by_id = base.join("by-id").join(entry.file_name());
        // Anything but exactly this shell's own by-id dir is skipped, never followed or deleted.
        let (resolved, want) = match (link.canonicalize(), by_id.canonicalize()) {
            (Ok(resolved), Ok(want)) => (resolved, want),
            // A stale link per launch leaves a dangling entry Quickshell never reaps.
            _ => {
                // Only its own missing by-id dir names a stale link; every other shape is kept.
                if std::fs::read_link(&link).ok().as_deref() == Some(by_id.as_path())
                    && matches!(std::fs::symlink_metadata(&by_id).map(|_| ()), Err(e) if e.kind() == std::io::ErrorKind::NotFound)
                {
                    let _ = std::fs::remove_file(&link);
                }
                continue;
            }
        };
        if resolved != want {
            continue;
        }
        // Owned by someone else is not Flea's to delete.
        if resolved.metadata().is_ok_and(|meta| !meta.is_dir() || meta.uid() != uid) {
            continue;
        }
        if !lock_dead(&resolved.join("instance.lock")) {
            continue;
        }
        let log_mtime = resolved.join("log.qslog").metadata().and_then(|meta| meta.modified()).unwrap_or(std::time::UNIX_EPOCH);
        dead.push(Dead { by_id, link, log_mtime });
    }
    // Newest first, so the truncation keeps the entries `qs log` can still reach.
    dead.sort_by_key(|a| std::cmp::Reverse(a.log_mtime));
    for victim in dead.iter().skip(KEEP_DEAD) {
        // The by-pid links first, then the by-shell link, then the verified by-id dir.
        for (link, target) in by_pid {
            if *target == victim.by_id {
                let _ = std::fs::remove_file(link);
            }
        }
        let _ = std::fs::remove_file(&victim.link);
        let _ = std::fs::remove_dir_all(&victim.by_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A classic F_GETLK does not see its own process's classic lock, but it does see an OFD lock.
    const F_OFD_SETLK: i32 = 37;

    // Sample fixture: by-id/<name>/ with its lock body and log, linked from by-shell and by-pid.
    fn make_entry(root: &Path, shell: &str, name: &str, lock_body: &[u8]) -> PathBuf {
        let dir = root.join("by-id").join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("instance.lock"), lock_body).unwrap();
        std::fs::write(dir.join("log.qslog"), "log").unwrap();
        std::os::unix::fs::symlink(&dir, root.join("by-shell").join(shell).join(name)).unwrap();
        std::os::unix::fs::symlink(&dir, root.join("by-pid").join(format!("9{name}"))).unwrap();
        dir
    }

    #[test]
    fn prune_keeps_the_newest_dead_and_nothing_it_should_not_touch() {
        let dir = crate::backend::testdir::TestDir::new("qsregistry");
        let root = dir.path();
        std::fs::create_dir_all(root.join("by-id")).unwrap();
        std::fs::create_dir_all(root.join("by-shell").join("flea")).unwrap();
        std::fs::create_dir_all(root.join("by-shell").join("omarchy")).unwrap();
        std::fs::create_dir_all(root.join("by-pid")).unwrap();
        // Sample fixture: by-id/spoof exists dead, but by-shell/flea/spoof points at by-id/target instead.
        let spoof_dir = root.join("by-id").join("spoof");
        std::fs::create_dir_all(&spoof_dir).unwrap();
        std::fs::write(spoof_dir.join("instance.lock"), b"pid 103\n").unwrap();
        std::fs::write(spoof_dir.join("log.qslog"), "log").unwrap();
        let target_dir = root.join("by-id").join("target");
        std::fs::create_dir_all(&target_dir).unwrap();
        std::fs::write(target_dir.join("instance.lock"), b"pid 104\n").unwrap();
        std::fs::write(target_dir.join("log.qslog"), "log").unwrap();
        std::os::unix::fs::symlink(&target_dir, root.join("by-shell").join("flea").join("spoof")).unwrap();
        // Oldest first with a sleep between, so the log mtimes stagger newest-last.
        for name in ["dead0", "dead1", "dead2", "dead3", "dead4", "dead5"] {
            make_entry(root, "flea", name, b"pid 100\n");
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        let live_dir = make_entry(root, "flea", "live", b"pid 101\n");
        let live_file = std::fs::OpenOptions::new().read(true).write(true).open(live_dir.join("instance.lock")).unwrap();
        let mut live_lock = Flock { l_type: F_WRLCK as i16, l_whence: 0, l_start: 0, l_len: 0, l_pid: 0 };
        use std::os::fd::AsRawFd;
        assert_eq!(unsafe { fcntl(live_file.as_raw_fd(), F_OFD_SETLK, &mut live_lock as *mut Flock) }, 0);
        make_entry(root, "flea", "starting", b"");
        let other_dir = make_entry(root, "omarchy", "other", b"pid 102\n");
        let outside = root.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("by-shell").join("flea").join("evil")).unwrap();
        prune_dead_in(root, &["flea"]);
        for gone in ["dead0", "dead1", "dead2"] {
            assert!(!root.join("by-id").join(gone).exists(), "{gone} was pruned");
            assert!(!root.join("by-shell").join("flea").join(gone).exists(), "{gone}'s shell link was pruned");
            assert!(!root.join("by-pid").join(format!("9{gone}")).exists(), "{gone}'s pid link was pruned");
        }
        for kept in ["dead3", "dead4", "dead5", "live", "starting"] {
            assert!(root.join("by-id").join(kept).is_dir(), "{kept} remains");
            assert!(root.join("by-shell").join("flea").join(kept).is_symlink(), "{kept}'s shell link remains");
            assert!(root.join("by-pid").join(format!("9{kept}")).is_symlink(), "{kept}'s pid link remains");
        }
        assert!(other_dir.is_dir(), "another shell's dead entry is untouched");
        assert!(root.join("by-shell").join("omarchy").join("other").is_symlink(), "another shell's link is untouched");
        assert!(root.join("by-shell").join("flea").join("evil").is_symlink(), "the outside link is untouched");
        assert!(outside.is_dir(), "the outside target is untouched");
        assert!(spoof_dir.is_dir(), "the divergent by-id dir is untouched");
        assert!(target_dir.is_dir(), "the divergent link target is untouched");
        assert_eq!(std::fs::read_link(root.join("by-shell").join("flea").join("spoof")).unwrap(), target_dir, "the divergent link is untouched");
        prune_dead_in(root, &["flea"]);
        assert_eq!(std::fs::read_dir(root.join("by-shell").join("flea")).unwrap().count(), 7, "a second call changes nothing");
        prune_dead_in(&root.join("no-such-base"), &["flea"]);
    }

    #[test]
    fn a_dangling_own_path_link_is_removed() {
        let dir = crate::backend::testdir::TestDir::new("qsregistry-stale");
        let root = dir.path();
        std::fs::create_dir_all(root.join("by-id")).unwrap();
        std::fs::create_dir_all(root.join("by-shell").join("flea")).unwrap();
        // Sample fixture: the link names its own missing by-id dir exactly.
        let missing = root.join("by-id").join("w73gone");
        std::os::unix::fs::symlink(&missing, root.join("by-shell").join("flea").join("w73gone")).unwrap();
        prune_dead_in(root, &["flea"]);
        assert!(std::fs::symlink_metadata(root.join("by-shell").join("flea").join("w73gone")).is_err(), "the stale link is gone");
        assert!(std::fs::symlink_metadata(&missing).is_err(), "nothing is created to remove it");
        prune_dead_in(root, &["flea"]);
        assert_eq!(std::fs::read_dir(root.join("by-shell").join("flea")).unwrap().count(), 0, "a second call changes nothing");
    }

    #[test]
    fn a_dangling_link_to_any_other_path_is_kept() {
        let dir = crate::backend::testdir::TestDir::new("qsregistry-stale-kept");
        let root = dir.path();
        std::fs::create_dir_all(root.join("by-id")).unwrap();
        std::fs::create_dir_all(root.join("by-shell").join("flea")).unwrap();
        std::fs::create_dir_all(root.join("by-shell").join("omarchy")).unwrap();
        // Sample fixture: an absolute target that is not its own by-id path, also missing.
        std::os::unix::fs::symlink(root.join("elsewhere-missing"), root.join("by-shell").join("flea").join("w73divergent")).unwrap();
        // Sample fixture: a relative raw target never equals the absolute by-id path.
        std::os::unix::fs::symlink(std::path::Path::new("w73relative"), root.join("by-shell").join("flea").join("w73relative")).unwrap();
        // Sample fixture: another shell's own-path dangling link is not this prune's to delete.
        std::os::unix::fs::symlink(root.join("by-id").join("w73foreign"), root.join("by-shell").join("omarchy").join("w73foreign")).unwrap();
        // Sample fixture: the by-id path itself exists as a dangling symlink, so it is not missing.
        std::os::unix::fs::symlink(root.join("no-such-target"), root.join("by-id").join("w73phantom")).unwrap();
        std::os::unix::fs::symlink(root.join("by-id").join("w73phantom"), root.join("by-shell").join("flea").join("w73phantom")).unwrap();
        // Sample fixture: a directory wearing a shell entry name is never a link to remove.
        std::fs::create_dir_all(root.join("by-shell").join("flea").join("w73direntry")).unwrap();
        prune_dead_in(root, &["flea"]);
        for kept in ["w73divergent", "w73relative", "w73phantom"] {
            assert!(root.join("by-shell").join("flea").join(kept).is_symlink(), "{kept} is kept");
        }
        assert!(root.join("by-shell").join("omarchy").join("w73foreign").is_symlink(), "another shell is untouched");
        assert!(std::fs::symlink_metadata(root.join("by-id").join("w73phantom")).is_ok(), "the phantom by-id link is untouched");
        assert!(root.join("by-shell").join("flea").join("w73direntry").is_dir(), "the directory entry is untouched");
        let count = std::fs::read_dir(root.join("by-shell").join("flea")).unwrap().count();
        prune_dead_in(root, &["flea"]);
        assert_eq!(std::fs::read_dir(root.join("by-shell").join("flea")).unwrap().count(), count, "a second call changes nothing");
    }
}
