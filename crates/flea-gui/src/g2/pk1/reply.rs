//! The portal reply's atomic write: a sibling created exclusively, synced, then renamed over the reply path.
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
const ATTEMPTS: usize = 16;

// The portal reads the reply only after this process exits, so the file it finds is whole or absent.
pub fn write(path: &Path, text: &str) -> io::Result<()> {
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the reply path names no file",
        ));
    };
    let parent = if parent.as_os_str().is_empty() {
        Path::new(".")
    } else {
        parent
    };
    let mut last = io::Error::new(io::ErrorKind::AlreadyExists, "no free temporary name");
    for _ in 0..ATTEMPTS {
        let mut temporary = PathBuf::from(parent);
        let serial = NEXT.fetch_add(1, Ordering::Relaxed);
        temporary.push(format!(
            ".{}.{}.{serial}.tmp",
            name.to_string_lossy(),
            std::process::id()
        ));
        // Created exclusively and owner-only, so a planted name or link is never written through.
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary);
        let mut file = match file {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                last = error;
                continue;
            }
            Err(error) => return Err(error),
        };
        let written = file
            .write_all(text.as_bytes())
            .and_then(|()| file.sync_all())
            .and_then(|()| fs::rename(&temporary, path));
        if written.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        return written;
    }
    Err(last)
}
