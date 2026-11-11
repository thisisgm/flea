use std::fs::File;
use std::os::fd::{AsRawFd, RawFd};

pub(crate) struct Lock {
    pub(super) file: File,
}

impl Lock {
    pub(super) fn new(file: File) -> Self {
        Self { file }
    }
}

impl AsRawFd for Lock {
    fn as_raw_fd(&self) -> RawFd {
        self.file.as_raw_fd()
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        // A fork may retain the file description after this owner closes its descriptor.
        if !super::try_flock(self, super::LOCK_UN) {
            eprintln!(
                "flea: AirDrop could not release its lock: {}",
                std::io::Error::last_os_error()
            );
        }
    }
}
