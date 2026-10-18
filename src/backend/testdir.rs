// Hard rule 9's sandbox, in code: every destructive test writes inside one of these and nowhere else.
use std::io::Write;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

// The one name that makes a directory deletable by this module; a directory without it is never touched.
const MARKER: &str = ".flea-test-sandbox";
// Every sandbox this process makes shares it, so a stray path outside the pattern is refused on the name alone.
const PREFIX: &str = "flea-test-";
// A sandbox lives under the temp root, which is at least two components deep, so a shallower path is a bug and not a root.
const MIN_COMPONENTS: usize = 3;

// Two tests in one process must not collide, and this crate takes no dependency that would generate a suffix.
static NEXT: AtomicUsize = AtomicUsize::new(0);

fn canonical_temp_root_allowed(root: &Path, home: &Path) -> bool {
    root.is_absolute() && home.is_absolute()
        && root.components().count() >= MIN_COMPONENTS - 1
        && !root.starts_with(home)
}

fn temporary_root() -> Option<PathBuf> {
    let root = std::env::temp_dir();
    let home = PathBuf::from(std::env::var_os("HOME")?);
    if !root.is_absolute() || !home.is_absolute() {
        return None;
    }
    let root = root.canonicalize().ok()?;
    let home = home.canonicalize().ok()?;
    (root.is_dir() && home.is_dir() && canonical_temp_root_allowed(&root, &home)).then_some(root)
}

// Created by the test itself, removed on drop, and only ever removable while its marker is inside it.
pub struct TestDir {
    path: PathBuf,
}

impl TestDir {
    // Panics rather than returning an error: a test that cannot make its sandbox must not go on to write anywhere.
    pub fn new(tag: &str) -> TestDir {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let name = format!("{}{}-{}-{}", PREFIX, tag, std::process::id(), n);
        let root = temporary_root().unwrap_or_else(|| panic!(
            "test temporary root {} must resolve to an absolute directory outside HOME", std::env::temp_dir().display()));
        let path = root.join(name);
        std::fs::create_dir(&path).expect("test sandbox could not be created");
        let mut marker = std::fs::File::create(path.join(MARKER)).expect("test sandbox marker");
        marker.write_all(b"flea test sandbox\n").expect("test sandbox marker");
        TestDir { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn assert_contains(&self, path: &Path) {
        assert!(removable(&self.path), "test sandbox is not owned: {}", self.path.display());
        assert!(!path.as_os_str().is_empty() && path.is_absolute()
            && path.starts_with(&self.path)
            && !path.components().any(|part| part == std::path::Component::ParentDir),
            "test path escapes sandbox: {}", path.display());
    }

    // Every test that names a file inside its sandbox goes through here, so no test builds a path by hand.
    pub fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }

    pub fn file(&self, name: &str, body: &str) -> PathBuf {
        let p = self.join(name);
        std::fs::write(&p, body).expect("test sandbox file");
        p
    }

    pub fn script(&self, name: &str, body: &str) -> PathBuf {
        self.executable(name, body.as_bytes(), 0o755)
    }

    // Written by a child process: the fork's copy of a held write fd is what made exec fail with ETXTBSY. Any file that will be exec'd goes through here.
    pub fn executable(&self, name: &str, body: &[u8], mode: u32) -> PathBuf {
        let p = self.join(name);
        self.assert_contains(&p);
        let mut writer = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg("cat > \"$1\" && chmod \"$2\" \"$1\"")
            .arg("sh")
            .arg(&p)
            .arg(format!("{mode:o}"))
            .stdin(std::process::Stdio::piped())
            .spawn()
            .expect("test script writer");
        writer.stdin.take().expect("test script writer stdin").write_all(body).expect("test script body");
        assert!(writer.wait().expect("test script writer exit").success(), "test script not written: {}", p.display());
        p
    }

    pub fn dir(&self, name: &str) -> PathBuf {
        let p = self.join(name);
        std::fs::create_dir_all(&p).expect("test sandbox dir");
        p
    }

    // Hold the old inode across unlink and creation so ext4 cannot reuse it, even without birth time.
    pub fn replace_file(&self, path: &Path, body: &str) {
        self.assert_contains(path);
        let held = std::fs::File::open(path).expect("hold the file being replaced");
        let old = held.metadata().expect("the file being replaced");
        std::fs::remove_file(path).expect("test sandbox remove");
        std::fs::write(path, body).expect("test sandbox file");
        let now = path.symlink_metadata().expect("the replacement");
        assert_ne!(old.ino(), now.ino(), "the replacement must have a new inode: {}", path.display());
        assert_eq!(std::fs::read_to_string(path).expect("read the replacement"), body, "the replacement must hold the new body: {}", path.display());
        drop(held);
    }
}

// The guard is here and not in the reviewer's head: a path that fails any clause is left on disk instead.
pub fn removable(path: &Path) -> bool {
    if !path.is_absolute() {
        return false;
    }
    if path.components().count() < MIN_COMPONENTS {
        return false;
    }
    let Some(root) = temporary_root() else { return false; };
    if !path.starts_with(root) || path.components().any(|part| part == std::path::Component::ParentDir) {
        return false;
    }
    match path.file_name().and_then(|n| n.to_str()) {
        Some(n) if n.starts_with(PREFIX) => {}
        _ => return false,
    }
    path.join(MARKER).is_file()
}

impl Drop for TestDir {
    fn drop(&mut self) {
        if !removable(&self.path) {
            return;
        }
        if std::fs::remove_dir_all(&self.path).is_err() {
            owner_can_write(&self.path);
            // Loud: a sandbox outliving its test is the state that left eleven roots on this box.
            if let Err(e) = std::fs::remove_dir_all(&self.path) {
                eprintln!("flea test sandbox left behind: {}: {}", self.path.display(), e);
            }
        }
    }
}

// A test is allowed to leave a directory its own owner cannot write into, and nothing can be unlinked
// from one of those, so the bits go back on rather than the sandbox outliving the test that made it.
fn owner_can_write(root: &Path) {
    // Opened rather than chmodded by name, because set_permissions and read_dir both follow a symlink
    // and one planted at a name this walk is about to take would carry it out of the sandbox entirely.
    let Ok(dir) = open_dir(root) else { return };
    let held = PathBuf::from(format!("/proc/self/fd/{}", dir.as_raw_fd()));
    let _ = std::fs::set_permissions(&held, std::fs::Permissions::from_mode(0o700));
    let Ok(entries) = std::fs::read_dir(&held) else { return };
    for entry in entries.flatten() {
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let child = entry.path();
        // A test may leave a directory with no read bit at all, and one of those cannot be opened to
        // be repaired; the name is under a parent this frame holds open, so the bits go back by name.
        if open_dir(&child).is_err() {
            let _ = std::fs::set_permissions(&child, std::fs::Permissions::from_mode(0o700));
        }
        owner_can_write(&child);
    }
}

// O_NOFOLLOW is what makes the walk above refuse a symlink outright rather than chmod its target.
fn open_dir(path: &Path) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(crate::oflags::O_DIRECTORY | crate::oflags::O_NOFOLLOW)
        .open(path)
}

// Forking threads per call; each one keeps forking while body runs, so a write in body overlaps some fork.
const SIBLING_FORKERS: usize = 4;
// How long each sibling child sits in pre_exec before it execs, so any fd it inherited stays open that long.
const SIBLING_HOLD: std::time::Duration = std::time::Duration::from_millis(10);

// Parallel test threads fork while a write fd is open, and each fork keeps that fd until its own exec. These siblings fork on purpose, each child held in pre_exec, so a write that overlaps a fork fails the next exec with ETXTBSY.
pub fn with_forking_siblings<T>(body: impl FnOnce() -> T) -> T {
    use std::os::unix::process::CommandExt;
    // Stops the siblings even when body panics, or the scope would wait on them forever.
    struct Stop<'a>(&'a AtomicBool);
    impl Drop for Stop<'_> {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Relaxed);
        }
    }
    let stop = AtomicBool::new(false);
    std::thread::scope(|scope| {
        for _ in 0..SIBLING_FORKERS {
            scope.spawn(|| {
                while !stop.load(Ordering::Relaxed) {
                    let mut child = std::process::Command::new("/bin/true");
                    // SAFETY: the hook only sleeps and touches no shared state, and it runs in the forked child before exec.
                    unsafe { child.pre_exec(|| { std::thread::sleep(SIBLING_HOLD); Ok(()) }); }
                    child.status().expect("a sibling fork runs");
                }
            });
        }
        let _stop = Stop(&stop);
        body()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // The repair for eleven leaked roots: without it this drop cannot unlink held.txt and the whole
    // sandbox stays on disk. Reverting the retry in Drop reddens exactly this assertion.
    #[test]
    fn a_sandbox_holding_a_directory_the_owner_cannot_write_is_still_removed() {
        let kept = {
            let d = TestDir::new("dropreadonly");
            let sub = d.dir("locked");
            std::fs::write(sub.join("held.txt"), "x").unwrap();
            // 0500 cannot be written into and 0000 cannot even be opened, and a test may leave either.
            let shut = d.dir("shut");
            std::fs::write(shut.join("inside.txt"), "x").unwrap();
            std::fs::set_permissions(&shut, std::fs::Permissions::from_mode(0o000)).unwrap();
            std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o500)).unwrap();
            d.path().to_path_buf()
        };
        assert!(!kept.exists(), "the sandbox goes with the test that made it: {}", kept.display());
    }

    // The walk is handed a name that is a symlink, which is the shape a planted /tmp entry takes, and
    // has to change nothing at the other end rather than chmodding its way down somebody else's tree.
    #[test]
    fn the_repair_walk_changes_nothing_through_a_symlink_it_is_handed() {
        let d = TestDir::new("dropsymlink");
        let target = d.dir("elsewhere");
        std::fs::write(target.join("inside.txt"), "x").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o500)).unwrap();
        let planted = d.join("planted");
        std::os::unix::fs::symlink(&target, &planted).unwrap();
        owner_can_write(&planted);
        let mode = target.symlink_metadata().unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o500, "a symlink is not the directory to repair, and its target is untouched");
    }

    #[test]
    fn temporary_roots_never_include_home_or_its_descendants() {
        let home = Path::new("/home/gm");
        assert!(canonical_temp_root_allowed(Path::new("/tmp"), home));
        assert!(canonical_temp_root_allowed(Path::new("/private/tmp"), Path::new("/Users/gm")));
        for path in ["/home/gm", "/home/gm/Work", "/home/gm/.config", "/home/gm/.local", "/home/gm/.cache", "/home/gm/tmp", "/home/gm/tmp/nested", "/", "", "relative"] {
            assert!(!canonical_temp_root_allowed(Path::new(path), home), "unsafe temporary root: {}", path);
        }
        assert!(!canonical_temp_root_allowed(Path::new("/tmp"), Path::new("relative")));
        assert!(!canonical_temp_root_allowed(Path::new("/tmp"), Path::new("/")));
    }

    #[test]
    fn a_sandbox_carries_its_marker_and_is_removable() {
        let d = TestDir::new("guard");
        assert!(d.path().is_dir());
        assert!(d.path().join(MARKER).is_file());
        assert!(removable(d.path()));
    }

    #[test]
    fn containment_refuses_empty_relative_outside_and_parent_paths() {
        let d = TestDir::new("containment");
        d.assert_contains(d.path());
        d.assert_contains(&d.join("missing/child"));
        for path in [PathBuf::new(), PathBuf::from("relative"), PathBuf::from("/"), d.join("../outside")] {
            assert!(std::panic::catch_unwind(|| d.assert_contains(&path)).is_err());
        }
    }

    #[test]
    fn the_guard_refuses_every_path_that_is_not_one_of_ours() {
        // A real directory with no marker: the case the incident actually needed refused.
        let outside = TestDir::new("outside");
        let plain = outside.dir("payload");
        assert!(!removable(&plain));
        // The home directory and its parents, named explicitly because they are what was lost.
        assert!(!removable(Path::new("/home/gm")));
        assert!(!removable(Path::new("/home")));
        assert!(!removable(Path::new("/")));
        assert!(!removable(Path::new("")));
        // A relative path can be anything the caller's cwd makes it, so it never qualifies.
        assert!(!removable(Path::new("flea-test-relative")));
        // Right shape, right place, no marker.
        let bare = outside.dir(&format!("{}bare", PREFIX));
        assert!(!removable(&bare));
    }

    #[test]
    fn a_replaced_path_reads_new_bytes_while_its_old_handle_keeps_old_bytes() {
        let d = TestDir::new("replacefile");
        let path = d.file("a.txt", "old");
        let mut held = std::fs::File::open(&path).unwrap();
        d.replace_file(&path, "new");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
        let mut old_body = String::new();
        std::io::Read::read_to_string(&mut held, &mut old_body).unwrap();
        assert_eq!(old_body, "old", "replacement must not rewrite the held file");
    }

    #[test]
    fn replacing_an_unheld_file_changes_its_inode_without_birth_time() {
        // Repeated replacements exercise freed-inode reuse without waiting for a birth-time tick.
        const REPLACEMENTS: usize = 64;
        let d = TestDir::new("replaceidentity");
        let path = d.file("a.txt", "old");
        // replace_file asserts the new inode and the new body on every pass.
        for _ in 0..REPLACEMENTS {
            d.replace_file(&path, "new");
        }
    }

    #[test]
    fn a_dropped_sandbox_takes_its_contents_with_it() {
        let kept;
        {
            let d = TestDir::new("drop");
            d.file("a.txt", "body");
            d.dir("sub");
            kept = d.path().to_path_buf();
            assert!(kept.is_dir());
        }
        assert!(!kept.exists());
    }
}
