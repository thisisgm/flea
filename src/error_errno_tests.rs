use super::*;

#[test]
#[rustfmt::skip] // one errno per row
fn every_legacy_errno_keeps_its_operation_path_and_sentence() {
    for (name, errno, expected) in [
        ("EIO", 5, "the drive could not read or write the data"),
        ("EBUSY", 16, "file or device is busy"),
        ("EXDEV", 18, "the source and destination are on different drives"),
        ("ENODEV", 19, "the drive is unavailable"),
        ("ENOTDIR", 20, "part of the path is not a folder"),
        ("EISDIR", 21, "item is a folder"),
        ("ENFILE", 23, "too many files are open"),
        ("EMFILE", 24, "too many files are open"),
        ("ETXTBSY", 26, "file or device is busy"),
        ("EFBIG", 27, "file is too large"),
        ("ENOSPC", 28, "the drive is full"),
        ("EROFS", 30, "the drive is read-only"),
        ("ENAMETOOLONG", 36, "that name is too long"),
        ("ENOTEMPTY", 39, "folder is not empty"),
        ("ELOOP", 40, "the path has too many links"),
        ("ESTALE", 116, "file is no longer available"),
        ("EDQUOT", 122, "storage quota exceeded"),
    ] {
        let error = std::io::Error::from_raw_os_error(errno);
        let mapped = from_io("copy", "/fixture/source", &error);
        assert_eq!(mapped.where_, "copy", "{name} ({errno})");
        assert_eq!(mapped.path, "/fixture/source", "{name} ({errno})");
        assert_eq!(mapped.msg, expected, "{name} ({errno})");
    }
}

#[test]
fn synthetic_newer_kinds_keep_the_legacy_fallback() {
    use std::io::{Error, ErrorKind};
    for kind in [
        ErrorKind::ResourceBusy,
        ErrorKind::CrossesDevices,
        ErrorKind::NotADirectory,
        ErrorKind::IsADirectory,
        ErrorKind::ExecutableFileBusy,
        ErrorKind::FileTooLarge,
        ErrorKind::StorageFull,
        ErrorKind::ReadOnlyFilesystem,
        ErrorKind::InvalidFilename,
        ErrorKind::DirectoryNotEmpty,
        ErrorKind::StaleNetworkFileHandle,
        ErrorKind::QuotaExceeded,
    ] {
        assert_eq!(
            io_message(&Error::from(kind)),
            "the operation could not finish",
            "{kind:?}"
        );
    }
}
