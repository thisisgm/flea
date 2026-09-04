use crate::backend::fd;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::process::{Child, ExitStatus};
use std::time::{Duration, Instant};

extern "C" {
    fn pidfd_open(pid: i32, flags: u32) -> i32;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ran {
    Succeeded,
    Failed,
    NotStarted,
}

// None is a deadline. Errors are infrastructure failures. The caller owns tree cleanup and reap.
pub fn wait_for_exit(child: &mut Child, limit: Duration) -> io::Result<Option<ExitStatus>> {
    let raw = unsafe { pidfd_open(child.id() as i32, 0) };
    if raw < 0 {
        return Err(io::Error::last_os_error());
    }
    let pidfd = unsafe { OwnedFd::from_raw_fd(raw) };
    let deadline = Instant::now() + limit;
    if fd::poll_until(pidfd.as_raw_fd(), deadline)? {
        child.wait().map(Some)
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_child_exit_is_returned_before_the_deadline() {
        let mut child = std::process::Command::new("/usr/bin/true").spawn().unwrap();
        assert!(wait_for_exit(&mut child, Duration::from_secs(20)).unwrap().unwrap().success());
    }

    #[test]
    fn a_running_child_reaches_the_deadline_without_being_reaped() {
        let mut child = std::process::Command::new("/usr/bin/sleep").arg("600").spawn().unwrap();
        assert!(wait_for_exit(&mut child, Duration::from_millis(100)).unwrap().is_none());
        child.kill().unwrap();
        child.wait().unwrap();
    }
}
