// Where ui.json lives and the one way it is rewritten: the lock, the re-read, the temp and the rename.
use crate::jsondoc::{self, Json};
use crate::oflags::O_NOFOLLOW;
use crate::uischema;
use crate::uistate;
use crate::userfile;
use std::fs;
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

const DIR: &str = "flea";
const FILE: &str = "ui.json";
const LOCK: &str = "ui.json.lock";
const LEGACY: &str = "view.json";
// What every refusal to touch ui.json leaves behind, named in the message.
const STATE_NOT_WRITTEN: &str = "the state file was not written";

// The state file is one user's, so it is created owner-only in the open() call and never chmod'd after.
const OWNER_ONLY_FILE: u32 = 0o600;
const OWNER_ONLY_DIR: u32 = 0o700;

// The one shared update path both front ends go through, and the paths it works on.
pub struct Store {
    file: PathBuf,
    lock: PathBuf,
    legacy: PathBuf,
}

impl Store {
    pub fn user() -> Result<Store, String> {
        Ok(Store::at(&state_home()?, &userfile::config_home()?))
    }

    pub fn at(state_dir: &Path, config_dir: &Path) -> Store {
        Store {
            file: state_dir.join(DIR).join(FILE),
            lock: state_dir.join(DIR).join(LOCK),
            legacy: config_dir.join(DIR).join(LEGACY),
        }
    }

    #[cfg(test)]
    pub fn file(&self) -> &Path {
        &self.file
    }

    #[cfg(test)]
    pub fn lock_file(&self) -> &Path {
        &self.lock
    }

    #[cfg(test)]
    pub fn legacy(&self) -> &Path {
        &self.legacy
    }

    // Never fails, so a front end can read it before first paint without a branch for a broken file.
    pub fn read(&self) -> Json {
        if let Ok(text) = fs::read_to_string(&self.file) {
            return if crate::persistdoc::parse(&text, crate::persistdoc::ui).is_ok() {
                uistate::from_file(&text)
            } else { uischema::defaults() };
        }
        // A ui.json that is there is the state file even when its bytes cannot be read, so 0.1.3's
        // view.json is read only while there is no ui.json at all.
        if fs::symlink_metadata(&self.file).is_ok() {
            return uischema::defaults();
        }
        match fs::read_to_string(&self.legacy) {
            Ok(text) => uistate::from_view_json(&text),
            Err(_) => uischema::defaults(),
        }
    }

    // The lock is held across the re-read, the validation, the merge, the temp write and the rename,
    // so a second Flea cannot land between this one's read and its write.
    pub fn update(&self, patch: &Json) -> Result<Json, String> {
        self.transform(|state| uistate::patched(state, patch))
    }

    pub fn transform(&self, change: impl FnOnce(&Json) -> Result<Json, String>) -> Result<Json, String> {
        let dir = self.file.parent().ok_or_else(|| format!("{} has no directory to write in", self.file.display()))?;
        make_dir(dir)?;
        let lock = take_lock(&self.lock)?;
        let next = change(&self.read())?;
        self.write(&next)?;
        unlock(&lock).map_err(|e| format!("{} could not be unlocked ({:?})", self.lock.display(), e.kind()))?;
        Ok(next)
    }

    // Before the window, because the window reads this file with its own FileView and applies no
    // schema of its own: an empty patch rewrites a document this can read through the same per-key
    // validation a patch gets, so a refused value cannot be what the first paint draws, while one it
    // cannot read is left alone and reads as the full default shape on both sides. It is also where
    // 0.1.3's view.json becomes ui.json, since read() falls back to it only while ui.json is absent.
    pub fn settle(&self) -> Result<(), String> {
        match fs::symlink_metadata(&self.file) {
            Ok(here) if here.file_type().is_file() && self.left_as_it_is() => Ok(()),
            Ok(_) => self.update(&Json::Obj(Vec::new())).map(|_| ()),
            // A first run with neither file writes nothing, the way a first run always has.
            Err(_) if fs::symlink_metadata(&self.legacy).is_err() => Ok(()),
            Err(_) => self.update(&Json::Obj(Vec::new())).map(|_| ()),
        }
    }

    // Whether this launch's settle leaves ui.json exactly as it is rather than rewriting it.
    fn left_as_it_is(&self) -> bool {
        match fs::read_to_string(&self.file) {
            Ok(text) => match jsondoc::parse(&text) {
                // A rewrite that would change nothing is not worth the launch: the lock, the temp, the
                // sync_all and the rename measured 6.7 to 18.6 ms a launch here, against 1.3 to 1.7 for one
                // that only reads.
                Ok(found) if crate::persistdoc::ui(&found) => text == jsondoc::render(&uistate::from_file(&text)),
                // A settle rewrite would spend the only copy of whatever the operator wrote and close
                // nothing, because both front ends already read this file as the full default shape.
                // Explicit writes refuse these bytes too, so no default replaces the only original.
                _ => true,
            },
            Err(_) => true,
        }
    }

    // A file that is not a JSON object is kept as ui.json.broken and replaced, as 0.3.12 did; a foreign or newer object is refused.
    fn write(&self, value: &Json) -> Result<(), String> {
        if let Some(text) = read_write_target(&self.file, STATE_NOT_WRITTEN)? {
            match jsondoc::parse(&text) {
                Ok(found) if found.as_object().is_some() => {
                    crate::persistdoc::parse(&text, crate::persistdoc::ui)
                        .map_err(|why| format!("{}: {}; file was left unchanged, so {STATE_NOT_WRITTEN}", self.file.display(), why))?;
                }
                _ => {
                    let backup = self.file.with_file_name("ui.json.broken");
                    read_write_target(&backup, STATE_NOT_WRITTEN)?;
                    replace(&backup, &text)?;
                    // The backup must survive a crash before the only original is replaced.
                    let dir = backup.parent().ok_or("ui.json.broken has no directory")?;
                    fs::File::open(dir).and_then(|file| file.sync_all())
                        .map_err(|e| format!("{} backup could not be synced ({:?}); {STATE_NOT_WRITTEN}", backup.display(), e.kind()))?;
                }
            }
        }
        replace(&self.file, &jsondoc::render(value))
    }
}

pub(crate) fn state_home() -> Result<PathBuf, String> {
    Ok(state_dir(userfile::env_dir("XDG_STATE_HOME"), &userfile::home()?))
}

// The environment is read by the caller above and never here, so the rule can be tested without
// mutating a process-wide variable that another test in this binary is reading at the same time.
fn state_dir(from_env: Option<PathBuf>, home: &Path) -> PathBuf {
    from_env.unwrap_or_else(|| home.join(".local").join("state"))
}

pub(crate) fn make_dir(dir: &Path) -> Result<(), String> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(OWNER_ONLY_DIR)
        .create(dir)
        .map_err(|e| format!("{} could not be created ({:?})", dir.display(), e.kind()))
}

// flock(2) numbers from Linux sys/file.h, the same on both supported architectures.
const LOCK_EXCLUSIVE: i32 = 2;
const LOCK_UNLOCK: i32 = 8;
const LOCK_NONBLOCKING: i32 = 4;

// std already links the system libc, so flock is declared here the way src/thp.rs declares prctl.
extern "C" {
    fn flock(fd: i32, operation: i32) -> i32;
}

// flock(2) through the declared symbol: advisory, exclusive, cross-process, and released when this file closes or the process dies.
pub(crate) fn take_lock(path: &Path) -> Result<fs::File, String> {
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(OWNER_ONLY_FILE)
        .custom_flags(O_NOFOLLOW)
        .open(path)
        .map_err(|e| format!("{} could not be opened as the ui.json lock ({:?})", path.display(), e.kind()))?;
    lock_exclusive(&file)
        .map_err(|e| format!("{} could not be locked ({:?})", path.display(), e.kind()))?;
    Ok(file)
}

// File::lock needs Rust 1.89 over the 1.77 floor; this is its single blocking exclusive flock.
pub(crate) fn lock_exclusive(file: &fs::File) -> std::io::Result<()> {
    use std::os::fd::AsRawFd;
    if unsafe { flock(file.as_raw_fd(), LOCK_EXCLUSIVE) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

// File::try_lock needs Rust 1.89 over the 1.77 floor; true is locked, false is held elsewhere.
pub fn try_lock_exclusive(file: &fs::File) -> std::io::Result<bool> {
    use std::os::fd::AsRawFd;
    if unsafe { flock(file.as_raw_fd(), LOCK_EXCLUSIVE | LOCK_NONBLOCKING) } == 0 {
        return Ok(true);
    }
    let failed = std::io::Error::last_os_error();
    if failed.kind() == std::io::ErrorKind::WouldBlock {
        return Ok(false);
    }
    Err(failed)
}

// File::unlock needs Rust 1.89 over the 1.77 floor; this is its flock release.
pub(crate) fn unlock(file: &fs::File) -> std::io::Result<()> {
    use std::os::fd::AsRawFd;
    if unsafe { flock(file.as_raw_fd(), LOCK_UNLOCK) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

// A link, a device or an unreadable state file is refused, never renamed over; outcome names the document kept.
pub(crate) fn read_write_target(path: &Path, outcome: &str) -> Result<Option<String>, String> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            Err(format!("{} is a symbolic link, so {outcome}", path.display()))
        }
        Ok(meta) if !meta.file_type().is_file() => {
            Err(format!("{} is not a regular file, so {outcome}", path.display()))
        }
        // read() answers the shipped defaults for bytes it cannot read, so this rename would put a default document over whatever the operator wrote.
        Ok(_) => fs::read_to_string(path).map(Some)
            .map_err(|_| format!("{} could not be read, so {outcome}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("{} could not be inspected ({:?}), so {outcome}", path.display(), error.kind())),
    }
}

// AGENTS.md "Predictable path writes": unlink this pid's own leftover, create exclusively, rename last.
pub(crate) fn replace(path: &Path, text: &str) -> Result<(), String> {
    let tmp = PathBuf::from(format!("{}.{}.tmp", path.display(), std::process::id()));
    let _ = fs::remove_file(&tmp);
    let written = write_new(&tmp, text).and_then(|()| {
        fs::rename(&tmp, path)
            .map_err(|e| format!("{} could not replace {} ({:?})", tmp.display(), path.display(), e.kind()))
    });
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    written
}

fn write_new(tmp: &Path, text: &str) -> Result<(), String> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(OWNER_ONLY_FILE)
        .open(tmp)
        .map_err(|e| format!("{} could not be created ({:?})", tmp.display(), e.kind()))?;
    file.write_all(text.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|e| format!("{} could not be written ({:?})", tmp.display(), e.kind()))
}

#[cfg(test)]
#[path = "uistore_tests.rs"]
mod tests;
