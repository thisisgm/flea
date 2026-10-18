// The flock every shared journal read-modify-write holds, and the deadline that turns a busy lock into contention.
use super::{validate_dir, Shared};
use std::os::unix::fs::OpenOptionsExt;

// Another window holds the lock for one read-modify-write, so a lock still busy past the deadline is contention to retry.
const LOCK_DEADLINE: std::time::Duration = std::time::Duration::from_secs(2);
const LOCK_POLL: std::time::Duration = std::time::Duration::from_millis(5);

// Held across one read-modify-write; dropping it releases the flock, so a dead process keeps nothing.
pub(super) struct Guard {
    _file: std::fs::File,
}

pub(super) fn lock(shared: &Shared) -> Result<Guard, String> {
    shared.contended.set(false);
    validate_dir(&shared.dir)?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(crate::oflags::O_NOFOLLOW)
        .open(shared.lock_path())
        .map_err(|e| format!("the shared undo lock could not be opened ({:?})", e.kind()))?;
    #[cfg(test)]
    crate::backend::renametiming::mark("flock_begin");
    let deadline = std::time::Instant::now() + LOCK_DEADLINE;
    loop {
        match crate::uistore::try_lock_exclusive(&file) {
            Ok(true) => break,
            Ok(false) => {}
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => {
                return Err(format!(
                    "the shared undo lock could not be taken ({:?})",
                    error.kind()
                ))
            }
        }
        if std::time::Instant::now() >= deadline {
            shared.contended.set(true);
            return Err(format!(
                "the shared undo lock stayed busy for {} seconds",
                LOCK_DEADLINE.as_secs()
            ));
        }
        std::thread::sleep(LOCK_POLL);
    }
    #[cfg(test)]
    crate::backend::renametiming::mark("flock_end");
    Ok(Guard { _file: file })
}
