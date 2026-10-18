// Hard rule 9's sandbox, in code: every destructive test writes inside one of these and nowhere else.
use std::io::Write;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

#[path = "testdir_init.rs"]
mod init;
use init::UnmarkedDir;

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
    allowed_root(&std::env::temp_dir())
}

pub(super) fn allowed_root(root: &Path) -> Option<PathBuf> {
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
    root: PathBuf,
}

impl TestDir {
    // Panics rather than returning an error: a test that cannot make its sandbox must not go on to write anywhere.
    pub fn new(tag: &str) -> TestDir {
        let root = temporary_root().unwrap_or_else(|| panic!(
            "test temporary root {} must resolve to an absolute directory outside HOME", std::env::temp_dir().display()));
        Self::new_in(tag, &root)
    }

    pub(super) fn new_in(tag: &str, root: &Path) -> TestDir {
        let root = allowed_root(root).expect("test sandbox root must be absolute and outside HOME");
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let name = format!("{}{}-{}-{}", PREFIX, tag, std::process::id(), n);
        let path = root.join(name);
        std::fs::create_dir(&path).expect("test sandbox could not be created");
        let rollback = UnmarkedDir::new(&path).inspect_err(|_| {
            if path.is_absolute() && !path.as_os_str().is_empty() {
                if let Err(error) = std::fs::remove_dir(&path) {
                    eprintln!("test sandbox initialization left a directory behind: {}: {error}", path.display());
                }
            }
        }).expect("hold the newly created test sandbox");
        let mut marker = rollback.marker().expect("test sandbox marker");
        marker.write_all(b"flea test sandbox\n").expect("test sandbox marker");
        rollback.disarm();
        TestDir { path, root }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn assert_contains(&self, path: &Path) {
        assert!(removable_in(&self.path, &self.root), "test sandbox is not owned: {}", self.path.display());
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

    // Written by a child process: the fork's copy of a held write fd is what made exec fail with ETXTBSY.
    pub fn script(&self, name: &str, body: &str) -> PathBuf {
        let p = self.join(name);
        self.assert_contains(&p);
        let mut writer = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg("cat > \"$1\" && chmod 755 \"$1\"")
            .arg("sh")
            .arg(&p)
            .stdin(std::process::Stdio::piped())
            .spawn()
            .expect("test script writer");
        writer.stdin.take().expect("test script writer stdin").write_all(body.as_bytes()).expect("test script body");
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
    temporary_root().is_some_and(|root| removable_in(path, &root))
}

fn removable_in(path: &Path, root: &Path) -> bool {
    if !path.is_absolute() {
        return false;
    }
    if path.components().count() < MIN_COMPONENTS {
        return false;
    }
    let Some(root) = allowed_root(root) else { return false; };
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
        if !removable_in(&self.path, &self.root) {
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

#[cfg(test)]
#[path = "testdir_tests.rs"]
mod tests;
