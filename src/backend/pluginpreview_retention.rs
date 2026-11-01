use std::fs::{File, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const LOCK_DEADLINE: Duration = Duration::from_secs(2);
const LOCK_POLL: Duration = Duration::from_millis(5);

// The shared file lock covers generation allocation, publication and eviction across all processes.
fn lock(root: &Path) -> Result<File, String> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(crate::oflags::O_NOFOLLOW | crate::oflags::O_NONBLOCK)
        .open(root.join("shown.lock"))
        .map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("plugin preview lock is not a regular file".into());
    }
    let deadline = Instant::now() + LOCK_DEADLINE;
    loop {
        match crate::uistore::try_lock_exclusive(&file) {
            Ok(true) => return Ok(file),
            Ok(false) => {}
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error.to_string()),
        }
        if Instant::now() >= deadline {
            return Err("plugin preview cache stayed busy".into());
        }
        std::thread::sleep(LOCK_POLL);
    }
}

pub(super) fn publish(input: &Path, root: &Path) -> Result<PathBuf, String> {
    let _lock = lock(root)?;
    let shown = root.join("shown");
    let mut files = Vec::new();
    for entry in std::fs::read_dir(&shown).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // Sample filename: 42.png; older PID-local names have no shared generation and retire first.
        if let Some(stem) = name.strip_suffix(".png") {
            files.push((stem.parse::<u64>().ok(), entry.path()));
        }
    }
    files.sort();
    let generation = files
        .iter()
        .filter_map(|(generation, _)| *generation)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or("plugin preview generation exhausted")?;
    let output = shown.join(format!("{generation}.png"));
    std::fs::rename(input, &output).map_err(|e| e.to_string())?;
    files.push((Some(generation), output.clone()));
    let extra = files.len().saturating_sub(super::KEPT);
    for (_, path) in files.into_iter().take(extra) {
        if path.is_absolute() && path.starts_with(&shown) {
            let _ = std::fs::remove_file(path);
        }
    }
    Ok(output)
}
