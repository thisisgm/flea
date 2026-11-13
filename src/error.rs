// Names the operation and the input, so the UI's message is enough to act on.
#[derive(Debug)]
pub struct FleaError {
    pub where_: String,
    pub path: String,
    pub msg: String,
}

pub fn from_io(where_: &str, path: &str, e: &std::io::Error) -> FleaError {
    FleaError {
        where_: where_.to_string(),
        path: path.to_string(),
        msg: io_message(e),
    }
}

pub fn io_message(error: &std::io::Error) -> String {
    use std::io::ErrorKind;
    if let Some(cause) = error.get_ref() {
        return match cause.downcast_ref::<std::io::Error>() {
            Some(inner) => io_message(inner),
            None => cause.to_string(),
        };
    }
    match error.kind() {
        ErrorKind::NotFound => "item not found",
        ErrorKind::PermissionDenied => "permission denied",
        ErrorKind::AlreadyExists => "already exists",
        ErrorKind::ConnectionRefused => "connection refused",
        ErrorKind::ConnectionReset => "connection reset",
        ErrorKind::ConnectionAborted => "connection ended",
        ErrorKind::NotConnected => "not connected",
        ErrorKind::AddrInUse => "address is already in use",
        ErrorKind::AddrNotAvailable => "address is unavailable",
        ErrorKind::BrokenPipe => "connection closed",
        ErrorKind::WouldBlock => "the item is busy; try again",
        ErrorKind::InvalidInput => "invalid input",
        ErrorKind::InvalidData => "invalid data",
        ErrorKind::TimedOut => "operation timed out",
        ErrorKind::WriteZero => "could not write all data",
        ErrorKind::Interrupted => "operation was interrupted",
        ErrorKind::Unsupported => "operation is not supported",
        ErrorKind::UnexpectedEof => "file ended unexpectedly",
        ErrorKind::OutOfMemory => "not enough memory",
        _ => linux_io_message(error.raw_os_error()),
    }.to_string()
}

// ponytail: Linux errno fills Rust 1.77's ErrorKind gaps; use named variants when the minimum compiler advances.
fn linux_io_message(code: Option<i32>) -> &'static str {
    const EIO: i32 = 5;
    const EBUSY: i32 = 16;
    const EXDEV: i32 = 18;
    const ENODEV: i32 = 19;
    const ENOTDIR: i32 = 20;
    const EISDIR: i32 = 21;
    const ENFILE: i32 = 23;
    const EMFILE: i32 = 24;
    const ETXTBSY: i32 = 26;
    const EFBIG: i32 = 27;
    const ENOSPC: i32 = 28;
    const EROFS: i32 = 30;
    const ENAMETOOLONG: i32 = 36;
    const ENOTEMPTY: i32 = 39;
    const ELOOP: i32 = 40;
    const ESTALE: i32 = 116;
    const EDQUOT: i32 = 122;
    match code {
        Some(EIO) => "the drive could not read or write the data",
        Some(EBUSY | ETXTBSY) => "file or device is busy",
        Some(EXDEV) => "the source and destination are on different drives",
        Some(ENODEV) => "the drive is unavailable",
        Some(ENOTDIR) => "part of the path is not a folder",
        Some(EISDIR) => "item is a folder",
        Some(ENFILE | EMFILE) => "too many files are open",
        Some(EFBIG) => "file is too large",
        Some(ENOSPC) => "the drive is full",
        Some(EROFS) => "the drive is read-only",
        Some(ENAMETOOLONG) => "that name is too long",
        Some(ENOTEMPTY) => "folder is not empty",
        Some(ELOOP) => "the path has too many links",
        Some(ESTALE) => "file is no longer available",
        Some(EDQUOT) => "storage quota exceeded",
        _ => "the operation could not finish",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carries_operation_path_and_message() {
        let io = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let e = from_io("list", "/root", &io);
        assert_eq!(e.where_, "list");
        assert_eq!(e.path, "/root");
        assert_eq!(e.msg, "denied");
    }

    #[test]
    fn common_io_causes_are_plain_words() {
        use std::io::{Error, ErrorKind};
        for (kind, expected) in [
            (ErrorKind::NotFound, "item not found"),
            (ErrorKind::PermissionDenied, "permission denied"),
            (ErrorKind::AlreadyExists, "already exists"),
            (ErrorKind::InvalidInput, "invalid input"),
            (ErrorKind::TimedOut, "operation timed out"),
            (ErrorKind::WouldBlock, "the item is busy; try again"),
            (ErrorKind::Other, "the operation could not finish"),
        ] {
            assert_eq!(io_message(&Error::from(kind)), expected);
        }
        assert_eq!(io_message(&Error::from_raw_os_error(i32::MAX)), "the operation could not finish");
    }

    #[test]
    fn wrapped_io_errors_do_not_leak_the_os_display() {
        let denied = std::io::Error::from_raw_os_error(13);
        let wrapped = std::io::Error::other(denied);
        assert_eq!(io_message(&wrapped), "permission denied");
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn filesystem_causes_missing_from_rust_177_are_named() {
        for (code, expected) in [
            (5, "the drive could not read or write the data"),
            (18, "the source and destination are on different drives"),
            (19, "the drive is unavailable"),
            (20, "part of the path is not a folder"),
            (28, "the drive is full"),
            (30, "the drive is read-only"),
            (36, "that name is too long"),
            (40, "the path has too many links"),
            (116, "file is no longer available"),
        ] {
            assert_eq!(io_message(&std::io::Error::from_raw_os_error(code)), expected);
        }
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn stale_disconnected_and_silent_shares_each_read_differently() {
        use std::io::{Error, ErrorKind};
        let stale = io_message(&Error::from_raw_os_error(116));
        let gone = io_message(&Error::from_raw_os_error(107));
        let silent = io_message(&Error::from_raw_os_error(110));
        assert_ne!(stale, gone, "ESTALE and ENOTCONN share no sentence");
        assert_ne!(stale, silent, "ESTALE and ETIMEDOUT share no sentence");
        assert_ne!(gone, silent, "ENOTCONN and ETIMEDOUT share no sentence");
        assert_eq!(gone, io_message(&Error::from(ErrorKind::NotConnected)));
        assert_eq!(silent, io_message(&Error::from(ErrorKind::TimedOut)));
    }
}
