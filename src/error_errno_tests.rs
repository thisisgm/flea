use super::*;

#[test]
fn every_legacy_errno_keeps_its_operation_path_and_sentence() {
    for (errno, expected) in [
        (5, "the drive could not read or write the data"),
        (16, "file or device is busy"),
        (18, "the source and destination are on different drives"),
        (19, "the drive is unavailable"),
        (20, "part of the path is not a folder"),
        (21, "item is a folder"),
        (23, "too many files are open"),
        (24, "too many files are open"),
        (26, "file or device is busy"),
        (27, "file is too large"),
        (28, "the drive is full"),
        (30, "the drive is read-only"),
        (36, "that name is too long"),
        (39, "folder is not empty"),
        (40, "the path has too many links"),
        (116, "file is no longer available"),
        (122, "storage quota exceeded"),
    ] {
        let error = std::io::Error::from_raw_os_error(errno);
        let mapped = from_io("copy", "/fixture/source", &error);
        assert_eq!(mapped.where_, "copy", "errno {errno}");
        assert_eq!(mapped.path, "/fixture/source", "errno {errno}");
        assert_eq!(mapped.msg, expected, "errno {errno}");
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
