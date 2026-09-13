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
        ErrorKind::NotFound => "file or folder not found",
        ErrorKind::PermissionDenied => "permission denied",
        ErrorKind::AlreadyExists => "already exists",
        ErrorKind::ConnectionRefused => "connection refused",
        ErrorKind::ConnectionReset => "connection reset",
        ErrorKind::ConnectionAborted => "connection ended",
        ErrorKind::NotConnected => "not connected",
        ErrorKind::AddrInUse => "address is already in use",
        ErrorKind::AddrNotAvailable => "address is unavailable",
        ErrorKind::BrokenPipe => "connection closed",
        ErrorKind::WouldBlock => "resource is temporarily unavailable",
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
        Some(EIO) => "input/output failed",
        Some(EBUSY | ETXTBSY) => "file or device is busy",
        Some(EXDEV) => "source and destination are on different filesystems",
        Some(ENODEV) => "device is unavailable",
        Some(ENOTDIR) => "a path component is not a folder",
        Some(EISDIR) => "item is a folder",
        Some(ENFILE | EMFILE) => "too many files are open",
        Some(EFBIG) => "file is too large",
        Some(ENOSPC) => "disk full",
        Some(EROFS) => "filesystem is read-only",
        Some(ENAMETOOLONG) => "file name is too long",
        Some(ENOTEMPTY) => "folder is not empty",
        Some(ELOOP) => "too many symbolic links",
        Some(ESTALE) => "file is no longer available",
        Some(EDQUOT) => "storage quota exceeded",
        _ => "input/output failed",
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
            (ErrorKind::NotFound, "file or folder not found"),
            (ErrorKind::PermissionDenied, "permission denied"),
            (ErrorKind::AlreadyExists, "already exists"),
            (ErrorKind::InvalidInput, "invalid input"),
            (ErrorKind::TimedOut, "operation timed out"),
            (ErrorKind::Other, "input/output failed"),
        ] {
            assert_eq!(io_message(&Error::from(kind)), expected);
        }
        assert_eq!(io_message(&Error::from_raw_os_error(i32::MAX)), "input/output failed");
    }

    #[test]
    fn wrapped_io_errors_do_not_leak_the_os_display() {
        let denied = std::io::Error::from_raw_os_error(13);
        let wrapped = std::io::Error::new(std::io::ErrorKind::Other, denied);
        assert_eq!(io_message(&wrapped), "permission denied");
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn filesystem_causes_missing_from_rust_177_are_named() {
        for (code, expected) in [
            (18, "source and destination are on different filesystems"),
            (20, "a path component is not a folder"),
            (28, "disk full"),
            (30, "filesystem is read-only"),
            (36, "file name is too long"),
            (40, "too many symbolic links"),
        ] {
            assert_eq!(io_message(&std::io::Error::from_raw_os_error(code)), expected);
        }
    }
}
