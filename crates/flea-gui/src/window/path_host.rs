//! The path jump's history file I/O, kept on workers and out of the path modules, which never touch the filesystem.
use crate::path::history;
use std::path::{Path, PathBuf};

/// When the history file last changed, its modification time and length; None while it is absent.
pub type Stamp = Option<(std::time::SystemTime, u64)>;

/// Far past any history Flea keeps, so a larger file is read as one that could not be read.
const MAX_HISTORY_BYTES: u64 = 4 << 20;

/// The history file's stamp, read on a worker so the next open rereads only a changed file.
pub fn stamp(file: &Path) -> Stamp {
    let meta = std::fs::metadata(file).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

/// One read for a worker: the stamp first, so a change landing during the read is seen by the next check.
pub fn load(file: &Path) -> (Stamp, Vec<String>) {
    let stamp = stamp(file);
    // Bounded and non-blocking, so a fifo or an endless file in its place never holds the worker.
    match flea::userfile::read_text(file, MAX_HISTORY_BYTES) {
        Ok(text) => (stamp, history::read(&text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (stamp, Vec::new()),
        // A failed read keeps no stamp, so the next open retries, as PickerRecent.qml warns on it.
        Err(error) => {
            eprintln!("flea: could not read {}: {error}", file.display());
            (None, Vec::new())
        }
    }
}

/// The desktop's recent file, named once from the environment the window started with.
pub fn file() -> &'static Path {
    static FILE: std::sync::LazyLock<PathBuf> = std::sync::LazyLock::new(|| {
        let home = std::env::var("HOME").unwrap_or_default();
        history::history_path(std::env::var("XDG_DATA_HOME").ok().as_deref(), &home)
    });
    &FILE
}
