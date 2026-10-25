// Publish an office render only over a recognized cache payload.
use std::io::Read;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub(super) fn publish(dest: &Path, mtime: SystemTime, body: &mut dyn Read) -> std::io::Result<()> {
    crate::cachewrite::require(dest)?;
    if let Some(parent) = dest.parent() {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)?;
    }
    let tmp = PathBuf::from(format!("{}.{}.tmp", dest.display(), std::process::id()));
    let _ = std::fs::remove_file(&tmp);
    let written = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)
        .and_then(|mut f| {
            std::io::copy(body, &mut f)?;
            f.set_modified(mtime)
        });
    match written.and_then(|()| std::fs::rename(&tmp, dest)) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}
