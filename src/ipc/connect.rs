use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::{ffi::OsStrExt, net::UnixStream};
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

const AF_UNIX: i32 = 1;
const SOCK_STREAM: i32 = 1;
const SOCK_NONBLOCK: i32 = 0o4000;
const SOCK_CLOEXEC: i32 = 0o2000000;
const EINPROGRESS: i32 = 115;
const EALREADY: i32 = 114;
const EISCONN: i32 = 106;
const RETRY_INTERVAL: Duration = Duration::from_millis(10);

#[repr(C)]
struct SockaddrUn {
    family: u16,
    path: [u8; 108],
}

extern "C" {
    fn socket(domain: i32, kind: i32, protocol: i32) -> i32;
    #[link_name = "connect"]
    fn connect_raw(fd: i32, address: *const SockaddrUn, length: u32) -> i32;
}

/// A zero timeout probes once; queued or backlogged connections are never reported as refusals.
pub fn with_timeout(path: &Path, timeout: Duration) -> io::Result<UnixStream> {
    let bytes = path.as_os_str().as_bytes();
    let mut address = SockaddrUn {
        family: AF_UNIX as u16,
        path: [0; 108],
    };
    if bytes.len() >= address.path.len() || bytes.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid IPC socket path",
        ));
    }
    address.path[..bytes.len()].copy_from_slice(bytes);
    let length = (std::mem::size_of::<u16>() + bytes.len() + 1) as u32;
    let deadline = Instant::now().checked_add(timeout).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "IPC connect timeout is too large",
        )
    })?;
    // Linux sets nonblocking and close-on-exec atomically before the first connection attempt.
    let raw = unsafe { socket(AF_UNIX, SOCK_STREAM | SOCK_NONBLOCK | SOCK_CLOEXEC, 0) };
    if raw < 0 {
        return Err(io::Error::last_os_error());
    }
    // The fresh descriptor has one owner and closes on every error path.
    let socket = unsafe { OwnedFd::from_raw_fd(raw) };
    loop {
        let result = unsafe { connect_raw(socket.as_raw_fd(), &address, length) };
        let error = if result < 0 {
            Some(io::Error::last_os_error())
        } else {
            None
        };
        if error
            .as_ref()
            .map_or(true, |error| error.raw_os_error() == Some(EISCONN))
        {
            let stream = UnixStream::from(socket);
            stream.set_nonblocking(false)?;
            return Ok(stream);
        }
        let error = error.unwrap();
        if !matches!(
            error.kind(),
            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
        ) && !matches!(error.raw_os_error(), Some(EINPROGRESS | EALREADY))
        {
            return Err(error);
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(timed_out());
        }
        // EAGAIN on AF_UNIX can mean a full queue without a pending connect, so retry connect itself.
        thread::sleep(remaining.min(RETRY_INTERVAL));
        if Instant::now() >= deadline {
            return Err(timed_out());
        }
    }
}

fn timed_out() -> io::Error {
    io::Error::new(io::ErrorKind::TimedOut, "IPC connection timed out")
}
