// The plugins folder's disk and process work: a folder copied in, a folder removed, a start answered.
use super::regfile;
use crate::oflags::O_NOFOLLOW;
use flea::ui_logic_plugin_settings::manifest::Invalid;
use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

// A plugin is a manifest, an executable and a few assets; a tree past these bounds is not one.
const MAX_ENTRIES: usize = 4096;
const MAX_DEPTH: usize = 16;
pub const MAX_BYTES: u64 = 64 << 20;
// One file streams through a buffer this size, never read whole.
pub const CHUNK: usize = 64 << 10;

#[derive(Default)]
struct Budget {
    entries: usize,
    bytes: u64,
    // A clone keeps every tracked file, so its dot entries count too.
    clone: bool,
}

impl Budget {
    fn entry(&mut self, from: &Path) -> Result<(), String> {
        self.entries += 1;
        if self.entries > MAX_ENTRIES {
            return Err(format!("{} holds too many files", from.display()));
        }
        Ok(())
    }

    fn bytes(&mut self, len: u64, from: &Path) -> Result<(), String> {
        self.bytes = self.bytes.saturating_add(len);
        if self.bytes > MAX_BYTES {
            return Err(format!(
                "{} is larger than {} MB",
                from.display(),
                MAX_BYTES >> 20
            ));
        }
        Ok(())
    }
}

// Walks a tree without writing, so an add refuses one past its bounds before anything lands.
pub fn measure(from: &Path) -> Result<(), String> {
    walk(from, None, 0, &mut Budget::default())
}

// A clone as it would land: every entry counts but the clone's own top-level .git.
pub fn measure_clone(from: &Path) -> Result<(), String> {
    walk(
        from,
        None,
        0,
        &mut Budget {
            clone: true,
            ..Budget::default()
        },
    )
}

// Regular files and folders only: a symlink in a shared folder could point the copy at anything.
pub fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    measure(from)?;
    walk(from, Some(to), 0, &mut Budget::default())
}

fn walk(from: &Path, to: Option<&Path>, depth: usize, budget: &mut Budget) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Err(format!("{} is nested too deeply", from.display()));
    }
    if let Some(to) = to {
        fs::create_dir(to).map_err(|e| format!("{}: {e}", to.display()))?;
    }
    for entry in fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        // A folder add copies no dot entry; a clone is measured with all of them but its own .git.
        let dot = name.as_bytes().starts_with(b".");
        if dot && (!budget.clone || (depth == 0 && name == ".git")) {
            continue;
        }
        budget.entry(from)?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        let target = to.map(|t| t.join(&name));
        if kind.is_dir() {
            walk(&entry.path(), target.as_deref(), depth + 1, budget)?;
        } else if kind.is_file() {
            let path = entry.path();
            let (file, meta) = open_member(&path)?;
            budget.bytes(meta.len(), from)?;
            if let Some(target) = target {
                let mode = meta.permissions().mode() & 0o755;
                let mut out = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(mode | 0o600)
                    .open(&target)
                    .map_err(|e| format!("{}: {e}", target.display()))?;
                // A file that grew since it was measured stops at its measured length plus one byte, and is refused.
                let copied = copy_stream(&mut file.take(meta.len() + 1), &mut out)
                    .map_err(|e| e.to_string())?;
                if copied != meta.len() {
                    return Err(format!("{} changed while it was copied", path.display()));
                }
            }
        }
    }
    Ok(())
}

// One file of a tree: no link followed, and O_NONBLOCK so an entry swapped for a fifo is refused instead of waited on.
pub fn open_member(path: &Path) -> Result<(fs::File, fs::Metadata), String> {
    regfile::open_if_regular_with_meta(path, O_NOFOLLOW)
        .map_err(|e| format!("{}: {e}", path.display()))
}

// Where a folder add records its folder: the store's own dot folder, one file per plugin, never inside a plugin.
pub const SOURCES: &str = ".sources";
pub const SOURCE_LIMIT: u64 = 4096;

pub fn read_source(store: &Path, id: &str) -> Option<String> {
    let path = store.join(SOURCES).join(id);
    // A link, a fifo or anything but a small regular file is no record.
    if !fs::symlink_metadata(&path).ok()?.is_file() {
        return None;
    }
    let file = regfile::open_if_regular(&path, O_NOFOLLOW).ok()?;
    let text = read_bounded(file, SOURCE_LIMIT)?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

// At most limit bytes and one more, so a longer source is refused having read only that much.
pub fn read_bounded(from: impl Read, limit: u64) -> Option<String> {
    let mut text = String::new();
    from.take(limit + 1).read_to_string(&mut text).ok()?;
    (text.len() as u64 <= limit).then_some(text)
}

// A predictable path, so the old record is unlinked first and the new one created exclusively.
pub fn write_source(store: &Path, id: &str, source: &str) -> Result<(), String> {
    let dir = store.join(SOURCES);
    match fs::DirBuilder::new().mode(0o700).create(&dir) {
        Err(e) if e.kind() != io::ErrorKind::AlreadyExists => {
            return Err(format!("{}: {e}", dir.display()))
        }
        _ => {}
    }
    if !fs::symlink_metadata(&dir)
        .map(|m| m.is_dir())
        .unwrap_or(false)
    {
        return Err(format!("{} is not a folder", dir.display()));
    }
    forget_source(store, id)?;
    let path = dir.join(id);
    let mut out = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .custom_flags(O_NOFOLLOW)
        .mode(0o600)
        .open(&path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    out.write_all(format!("{source}\n").as_bytes())
        .map_err(|e| e.to_string())
}

pub fn forget_source(store: &Path, id: &str) -> Result<(), String> {
    match fs::remove_file(store.join(SOURCES).join(id)) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e.to_string()),
        _ => Ok(()),
    }
}

pub fn copy_stream(from: &mut impl Read, to: &mut impl Write) -> io::Result<u64> {
    let mut buf = [0u8; CHUNK];
    let mut total = 0;
    loop {
        let n = match from.read(&mut buf) {
            Ok(0) => return Ok(total),
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        to.write_all(&buf[..n])?;
        total += n as u64;
    }
}

// Waits for a child up to its deadline and kills it past that, so a hung network never holds the request.
pub(super) fn finish(mut child: std::process::Child, deadline: Duration) -> Result<(), String> {
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => return Err(format!("exit {}", status.code().unwrap_or(-1))),
            Ok(None) if started.elapsed() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("timed out".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return Err(e.to_string()),
        }
    }
}

// Only a folder directly inside the plugins folder, named and not a link, is ever deleted.
pub fn remove(root: &Path, folder: &Path) -> Result<(), String> {
    let name = folder.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let inside = folder.is_absolute()
        && root.is_absolute()
        && folder.parent() == Some(root)
        && !name.is_empty()
        && name != "."
        && name != "..";
    if !inside {
        return Err(format!(
            "{} is not inside {}",
            folder.display(),
            root.display()
        ));
    }
    let meta = fs::symlink_metadata(folder).map_err(|e| e.to_string())?;
    if meta.file_type().is_symlink() {
        return fs::remove_file(folder).map_err(|e| e.to_string());
    }
    fs::remove_dir_all(folder).map_err(|e| e.to_string())
}

// The entry point resolved inside its own folder, and an executable regular file there.
pub fn entry(folder: &Path, run: &str) -> Result<PathBuf, Invalid> {
    let bad = || Invalid::new("exec", run);
    let base = folder.canonicalize().map_err(|_| bad())?;
    let path = folder.join(run).canonicalize().map_err(|_| bad())?;
    let meta = fs::metadata(&path).map_err(|_| bad())?;
    if !path.starts_with(&base) || !meta.is_file() || meta.permissions().mode() & 0o111 == 0 {
        return Err(bad());
    }
    Ok(path)
}
