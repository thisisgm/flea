//! Legacy Network edits re-read under a file lock and publish an atomic replacement off-thread.
use flea::{backend::regfile, ui_logic_rail_places::places};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::{
        fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
        io::AsRawFd,
    },
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Clone, Debug)]
pub enum Edit {
    Rename {
        uri: String,
        label: String,
    },
    Replace {
        old: String,
        uri: String,
        label: String,
    },
    Remove {
        uri: String,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    Read(&'static str),
    Write,
}
impl Failure {
    pub fn key(&self) -> &'static str {
        match self {
            Self::Read(key) => key,
            Self::Write => "network.saved_write_failed",
        }
    }
}

pub fn updated(body: &str, edit: &Edit) -> String {
    match edit {
        Edit::Rename { uri, label } => places::relabel(body, uri, label),
        Edit::Replace { old, uri, label } => places::replace(body, old, uri, label),
        Edit::Remove { uri } => flea::ui_logic_network_shares::places::remove_bookmark(body, uri),
    }
}

/// The caller supplies its config path; no read or write happens until an edit is submitted.
pub fn submit(path: PathBuf, edit: Edit) -> async_channel::Receiver<Result<bool, Failure>> {
    let (send, receive) = async_channel::bounded(1);
    let result_send = send.clone();
    if std::thread::Builder::new()
        .name("flea-network-save".into())
        .spawn(move || {
            let _ = result_send.send_blocking(write(&path, &edit));
        })
        .is_err()
    {
        let _ = send.try_send(Err(Failure::Write));
    }
    receive
}

fn write(path: &Path, edit: &Edit) -> Result<bool, Failure> {
    let target = match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            fs::canonicalize(path).map_err(|_| read_failure(edit))?
        }
        Ok(_) => path.to_path_buf(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => path.to_path_buf(),
        Err(_) => return Err(read_failure(edit)),
    };
    let parent = target.parent().ok_or(Failure::Write)?;
    // A first save creates the GTK config folder, as GTK's own file chooser does.
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)
        .map_err(|_| Failure::Write)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(parent.join(".flea-network.lock"))
        .map_err(|_| Failure::Write)?;
    // SAFETY: flock holds only the opened per-directory lock for the lifetime of this File.
    if unsafe { flock(lock.as_raw_fd(), 2) } != 0 {
        return Err(Failure::Write);
    }
    let (body, mode) = match regfile::open_if_regular_with_meta(&target, 0) {
        Ok((file, meta)) => {
            let mut bytes = Vec::new();
            file.take(1_048_577)
                .read_to_end(&mut bytes)
                .map_err(|_| read_failure(edit))?;
            if bytes.len() > 1_048_576 {
                return Err(read_failure(edit));
            }
            (
                String::from_utf8(bytes).map_err(|_| read_failure(edit))?,
                meta.permissions().mode() & 0o777,
            )
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (String::new(), 0o600),
        Err(_) => return Err(read_failure(edit)),
    };
    let next = updated(&body, edit);
    if next == body {
        return Ok(false);
    }
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let scratch = parent.join(format!(
        ".flea-bookmarks-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(&scratch)
        .map_err(|_| Failure::Write)?;
    let result = (|| {
        output.write_all(next.as_bytes())?;
        output.sync_all()?;
        fs::rename(&scratch, &target)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&scratch);
        return Err(Failure::Write);
    }
    Ok(true)
}
extern "C" {
    fn flock(fd: i32, operation: i32) -> i32;
}

fn read_failure(edit: &Edit) -> Failure {
    Failure::Read(match edit {
        Edit::Rename { .. } => "network.saved_rename_read_failed",
        Edit::Replace { .. } => "network.saved_replace_read_failed",
        Edit::Remove { .. } => "network.saved_remove_read_failed",
    })
}
