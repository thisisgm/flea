use std::io;
use std::path::Path;

const EEXIST: i32 = 17;
const EINVAL: i32 = 22;
const ENOSYS: i32 = 38;
const EOPNOTSUPP: i32 = 95;

pub(super) fn fallback(from: &Path, to: &Path, error: io::Error) -> io::Result<()> {
    fallback_with_lookup(from, to, error, |path| path.symlink_metadata())
}

fn fallback_with_lookup(
    from: &Path,
    to: &Path,
    error: io::Error,
    lookup: impl FnOnce(&Path) -> io::Result<std::fs::Metadata>,
) -> io::Result<()> {
    if !matches!(error.raw_os_error(), Some(EINVAL | ENOSYS | EOPNOTSUPP)) {
        return Err(error);
    }
    match lookup(to) {
        Ok(_) => Err(io::Error::from_raw_os_error(EEXIST)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => std::fs::rename(from, to),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;
    use std::os::unix::fs::{symlink, MetadataExt};

    fn retry(from: &Path, to: &Path, errno: i32) -> io::Result<()> {
        fallback(from, to, io::Error::from_raw_os_error(errno))
    }

    #[test]
    fn unsupported_flags_move_files_trees_and_links_without_copying() {
        let sandbox = TestDir::new("checkedrename");
        let file = sandbox.file("source.txt", "body");
        let directory = sandbox.dir("source-dir");
        std::fs::write(directory.join("child"), "nested").unwrap();
        let child_inode = directory.join("child").metadata().unwrap().ino();
        let link = sandbox.join("source-link");
        symlink("missing", &link).unwrap();
        let destination = sandbox.dir("destination");
        for errno in [EINVAL, ENOSYS, EOPNOTSUPP] {
            for source in [&file, &directory, &link] {
                let target = destination.join(source.file_name().unwrap());
                let inode = source.symlink_metadata().unwrap().ino();
                retry(source, &target, errno).unwrap();
                assert!(source.symlink_metadata().is_err());
                assert_eq!(target.symlink_metadata().unwrap().ino(), inode);
                retry(&target, source, errno).unwrap();
                assert!(target.symlink_metadata().is_err());
                assert_eq!(source.symlink_metadata().unwrap().ino(), inode);
            }
        }
        assert_eq!(directory.join("child").metadata().unwrap().ino(), child_inode);
        assert_eq!(std::fs::read_to_string(file).unwrap(), "body");
        assert_eq!(std::fs::read_link(link).unwrap(), Path::new("missing"));
    }

    #[test]
    fn existing_files_directories_and_dangling_links_are_refused() {
        let sandbox = TestDir::new("checkedrenamecollision");
        let source = sandbox.file("source", "source body");
        let file = sandbox.file("target-file", "target body");
        let directory = sandbox.dir("target-dir");
        let link = sandbox.join("target-link");
        symlink("missing", &link).unwrap();
        for errno in [EINVAL, ENOSYS, EOPNOTSUPP] {
            for target in [&file, &directory, &link] {
                let inode = target.symlink_metadata().unwrap().ino();
                assert_eq!(retry(&source, target, errno).unwrap_err().raw_os_error(), Some(EEXIST));
                assert_eq!(target.symlink_metadata().unwrap().ino(), inode);
                assert_eq!(std::fs::read_to_string(&source).unwrap(), "source body");
            }
        }
        assert_eq!(std::fs::read_to_string(file).unwrap(), "target body");
        assert_eq!(std::fs::read_link(link).unwrap(), Path::new("missing"));
    }

    #[test]
    fn other_errors_never_retry_even_with_an_absent_destination() {
        let sandbox = TestDir::new("checkedrenamepolicy");
        let source = sandbox.file("source", "body");
        let target = sandbox.join("target");
        for errno in [1, 2, 5, 13, 17, 18, 20, 28, 30, 39] {
            assert_eq!(retry(&source, &target, errno).unwrap_err().raw_os_error(), Some(errno));
        }
        assert!(fallback(&source, &target, io::Error::new(io::ErrorKind::Other, "failure")).is_err());
        assert_eq!(std::fs::read_to_string(source).unwrap(), "body");
        assert!(!target.exists());
    }

    #[test]
    fn failed_destination_lookup_or_native_rename_leaves_the_source() {
        let sandbox = TestDir::new("checkedrenamefailure");
        let source = sandbox.file("source", "body");
        let blocked = sandbox.dir("blocked");
        for errno in [EINVAL, ENOSYS, EOPNOTSUPP] {
            let target = blocked.join("target");
            let result = fallback_with_lookup(
                &source, &target, io::Error::from_raw_os_error(errno), |path| {
                    assert_eq!(path, target);
                    Err(io::Error::from(io::ErrorKind::PermissionDenied))
                },
            );
            assert_eq!(result.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
            assert_eq!(retry(&source, &sandbox.join("missing/target"), errno)
                .unwrap_err().kind(), io::ErrorKind::NotFound);
            assert_eq!(retry(&blocked, &blocked.join("inside"), errno)
                .unwrap_err().raw_os_error(), Some(EINVAL));
        }
        assert_eq!(std::fs::read_to_string(source).unwrap(), "body");
        assert!(!blocked.join("target").exists());
        assert!(!blocked.join("inside").exists());
    }
}