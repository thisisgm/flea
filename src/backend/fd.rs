use std::io::{self, Read};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::time::Instant;

const EINTR: i32 = 4;
const F_GETFD: i32 = 1;
const F_SETFD: i32 = 2;
const FD_CLOEXEC: i32 = 1;
const O_CLOEXEC: i32 = 0o2000000;
const POLLIN: i16 = 0x001;
const POLLERR: i16 = 0x008;
const POLLHUP: i16 = 0x010;
const POLLNVAL: i16 = 0x020;
const NS_PER_MS: u128 = 1_000_000;

#[repr(C)]
struct PollFd {
    fd: RawFd,
    events: i16,
    revents: i16,
}

extern "C" {
    fn pipe2(fds: *mut RawFd, flags: i32) -> i32;
    fn fcntl(fd: RawFd, command: i32, argument: i32) -> i32;
    fn dup2(oldfd: RawFd, newfd: RawFd) -> i32;
    fn poll(fds: *mut PollFd, count: usize, timeout: i32) -> i32;
}

pub struct Pipe {
    pub read: OwnedFd,
    pub write: OwnedFd,
}

pub fn pipe() -> io::Result<Pipe> {
    let mut fds = [-1; 2];
    if unsafe { pipe2(fds.as_mut_ptr(), O_CLOEXEC) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(Pipe { read: unsafe { OwnedFd::from_raw_fd(fds[0]) }, write: unsafe { OwnedFd::from_raw_fd(fds[1]) } })
}

// Command::spawn calls this only in its new child. The parent and every other child keep CLOEXEC.
pub unsafe fn inherit_on_exec(fd: RawFd) -> io::Result<()> {
    let flags = fcntl(fd, F_GETFD, 0);
    if flags < 0 || fcntl(fd, F_SETFD, flags & !FD_CLOEXEC) < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub fn close_on_exec(fd: RawFd) -> io::Result<()> {
    let flags = unsafe { fcntl(fd, F_GETFD, 0) };
    if flags < 0 || unsafe { fcntl(fd, F_SETFD, flags | FD_CLOEXEC) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub fn duplicate_to(fd: RawFd, target: RawFd) -> io::Result<()> {
    if unsafe { dup2(fd, target) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub fn poll_until(fd: RawFd, deadline: Instant) -> io::Result<bool> {
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let timeout = left.as_nanos().div_ceil(NS_PER_MS).min(i32::MAX as u128) as i32;
        let mut item = PollFd { fd, events: POLLIN, revents: 0 };
        let ready = unsafe { poll(&mut item, 1, timeout) };
        if ready > 0 && item.revents & (POLLIN | POLLHUP) != 0 {
            return Ok(true);
        }
        if ready == 0 {
            return Ok(false);
        }
        if ready < 0 && io::Error::last_os_error().raw_os_error() == Some(EINTR) {
            continue;
        }
        if ready > 0 && item.revents & (POLLERR | POLLNVAL) != 0 {
            return Err(io::Error::other("descriptor poll failed"));
        }
        return Err(io::Error::last_os_error());
    }
}

pub fn read_exact_until(input: &mut (impl Read + AsRawFd), mut bytes: &mut [u8], deadline: Instant) -> io::Result<()> {
    while !bytes.is_empty() {
        wait_readable(input.as_raw_fd(), deadline)?;
        match input.read(bytes) {
            Ok(0) => return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "pipe closed")),
            Ok(count) => bytes = &mut bytes[count..],
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

pub fn read_to_end_until(input: &mut (impl Read + AsRawFd), max: usize, deadline: Instant) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    loop {
        wait_readable(input.as_raw_fd(), deadline)?;
        let mut chunk = [0u8; 512];
        match input.read(&mut chunk) {
            Ok(0) => return Ok(bytes),
            Ok(count) if bytes.len() + count <= max => bytes.extend_from_slice(&chunk[..count]),
            Ok(_) => return Err(io::Error::other("pipe response is too large")),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
}

fn wait_readable(fd: RawFd, deadline: Instant) -> io::Result<()> {
    if poll_until(fd, deadline)? {
        Ok(())
    } else {
        Err(io::Error::new(io::ErrorKind::TimedOut, "pipe response deadline"))
    }
}
