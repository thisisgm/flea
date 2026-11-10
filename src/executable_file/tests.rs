use super::*;
use crate::backend::testdir::TestDir;
use std::io::Cursor;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::fs::PermissionsExt;

struct Sibling {
    release: Option<OwnedFd>,
    pid: libc::pid_t,
}

impl Sibling {
    fn fork() -> Self {
        let mut pipe = [0; 2];
        assert_eq!(
            unsafe {
                libc::socketpair(
                    libc::AF_UNIX,
                    libc::SOCK_STREAM | libc::SOCK_CLOEXEC,
                    0,
                    pipe.as_mut_ptr(),
                )
            },
            0
        );
        let pid = unsafe { libc::fork() };
        assert!(pid >= 0);
        if pid == 0 {
            // Only async-signal-safe calls run in the forked child.
            unsafe {
                libc::close(pipe[1]);
                let mut byte = 0u8;
                libc::read(pipe[0], (&mut byte as *mut u8).cast(), 1);
                libc::_exit(0);
            }
        }
        unsafe {
            libc::close(pipe[0]);
        }
        Self {
            release: Some(unsafe { OwnedFd::from_raw_fd(pipe[1]) }),
            pid,
        }
    }
}

impl Drop for Sibling {
    fn drop(&mut self) {
        if let Some(release) = &self.release {
            unsafe {
                libc::shutdown(release.as_raw_fd(), libc::SHUT_RDWR);
            }
        }
        drop(self.release.take());
        unsafe {
            libc::waitpid(self.pid, std::ptr::null_mut(), 0);
        }
    }
}

struct ForkingRead {
    body: Cursor<Vec<u8>>,
    sibling: Option<Sibling>,
}

impl Read for ForkingRead {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if self.sibling.is_none() {
            self.sibling = Some(Sibling::fork());
        }
        self.body.read(bytes)
    }
}

#[test]
fn executable_copy_cannot_leak_a_writer_to_a_sibling_fork() {
    let dir = TestDir::new("executable-fork");
    let target = dir.join("run");
    let body = b"#!/bin/sh\nexit 0\n".to_vec();
    let mut reader = ForkingRead {
        body: Cursor::new(body.clone()),
        sibling: None,
    };
    copy_new(&target, &mut reader, body.len() as u64, 0o700).unwrap();
    let status = Command::new(&target)
        .status()
        .expect("executable starts while sibling is still blocked before exec");
    assert!(status.success());
    assert!(reader.sibling.is_some());
}

#[test]
fn executable_replacement_does_not_reopen_the_old_executing_inode() {
    let dir = TestDir::new("executable-replace");
    let target = dir.join("run");
    write(&target, b"#!/bin/sh\nexit 0\n", 0o700).unwrap();
    let old_writer = fs::OpenOptions::new().write(true).open(&target).unwrap();
    let sibling = Sibling::fork();
    drop(old_writer);
    assert_eq!(
        Command::new(&target).status().unwrap_err().raw_os_error(),
        Some(libc::ETXTBSY)
    );
    write(&target, b"#!/bin/sh\nexit 7\n", 0o700).unwrap();
    assert_eq!(Command::new(&target).status().unwrap().code(), Some(7));
    drop(sibling);
}

#[test]
fn executable_copy_preserves_existing_entries_and_refuses_changed_sources() {
    let dir = TestDir::new("executable-refuse");
    let existing = dir.file("existing", "keep");
    let outside = dir.file("outside", "untouched");
    let link = dir.join("link");
    std::os::unix::fs::symlink(&outside, &link).unwrap();
    for target in [&existing, &link] {
        assert_eq!(
            copy_new(target, &mut &b"new"[..], 3, 0o700)
                .unwrap_err()
                .kind(),
            io::ErrorKind::AlreadyExists
        );
    }
    for length in [2, 4] {
        let target = dir.join("changed");
        assert_eq!(
            copy_new(&target, &mut &b"new"[..], length, 0o700)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
        assert!(!target.exists());
    }
    assert_eq!(fs::read_to_string(&existing).unwrap(), "keep");
    assert_eq!(fs::read_to_string(&outside).unwrap(), "untouched");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 4);
}

#[test]
fn executable_writer_errors_do_not_publish_or_leave_temporary_files() {
    struct Broken;
    impl Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("source failed"))
        }
    }
    let dir = TestDir::new("executable-error");
    let target = dir.file("run", "old");
    assert_eq!(
        publish(&target, &mut Broken, 1, 0o700, true)
            .unwrap_err()
            .to_string(),
        "source failed"
    );
    assert_eq!(fs::read_to_string(&target).unwrap(), "old");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    let error = write(&target, b"new", u32::MAX).unwrap_err();
    assert!(error.to_string().contains("executable writer exited"));
    assert!(error
        .to_string()
        .starts_with(&format!("{}: ", target.display())));
    assert_eq!(fs::read_to_string(&target).unwrap(), "old");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    write(&target, b"#!/bin/sh\nexit 0\n", 0o700).unwrap();
    assert_eq!(
        fs::metadata(target).unwrap().permissions().mode() & 0o777,
        0o700
    );
}

#[test]
fn executable_mode_keeps_the_process_umask() {
    // Sample line from /proc/self/status: "Umask:\t0022".
    let status = fs::read_to_string("/proc/self/status").unwrap();
    let umask = status
        .lines()
        .find_map(|line| line.strip_prefix("Umask:"))
        .map(|value| u32::from_str_radix(value.trim(), 8).unwrap())
        .unwrap();
    assert_ne!(
        umask & 0o777,
        0,
        "the host umask masks no bit, so this test proves nothing"
    );
    let dir = TestDir::new("executable-umask");
    let target = dir.join("run");
    write(&target, b"#!/bin/sh\nexit 0\n", 0o777).unwrap();
    assert_eq!(
        fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o777 & !umask
    );
}

#[test]
fn unsupported_noreplace_falls_back_to_an_exclusive_link() {
    let dir = TestDir::new("executable-fallback");
    let body = dir.file("body", "new");
    let existing = dir.file("existing", "keep");
    for errno in [EINVAL, ENOSYS, EOPNOTSUPP] {
        let unsupported = || Err(io::Error::from_raw_os_error(errno));
        assert_eq!(
            exclusive_fallback(unsupported(), &body, &existing)
                .unwrap_err()
                .kind(),
            io::ErrorKind::AlreadyExists
        );
        let fresh = dir.join(&format!("fresh-{errno}"));
        exclusive_fallback(unsupported(), &body, &fresh).unwrap();
        assert_eq!(fs::read_to_string(&fresh).unwrap(), "new");
    }
    assert_eq!(fs::read_to_string(&existing).unwrap(), "keep");
    let denied = dir.join("denied");
    let error = io::Error::from_raw_os_error(libc::EACCES);
    assert_eq!(
        exclusive_fallback(Err(error), &body, &denied)
            .unwrap_err()
            .raw_os_error(),
        Some(libc::EACCES)
    );
    assert!(!denied.exists());
}
