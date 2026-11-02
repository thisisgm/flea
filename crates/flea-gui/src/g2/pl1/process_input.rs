//! A secret written in bounded nonblocking chunks, wiped when stdin closes on any path.
use super::Secret;
use std::io::{self, ErrorKind, Write};
use std::os::fd::AsRawFd;
use std::process::ChildStdin;

pub(super) struct Input {
    stream: ChildStdin,
    secret: Secret,
    offset: usize,
}

impl Input {
    pub(super) fn new(stream: Option<ChildStdin>, secret: Option<Secret>) -> Option<Self> {
        let (stream, secret) = stream.zip(secret)?;
        // If setup fails, close stdin and wipe rather than risk a blocking write.
        nonblocking(&stream).ok()?;
        Some(Self {
            stream,
            secret,
            offset: 0,
        })
    }

    pub(super) fn fd(&self) -> i32 {
        self.stream.as_raw_fd()
    }

    pub(super) fn withheld(&self, withhold: bool) -> bool {
        withhold && self.offset == 0
    }

    /// Returns true when the buffer is finished or the reader has gone away, including EPIPE.
    pub(super) fn write(&mut self) -> bool {
        if self.offset == self.secret.expose().len() {
            return true;
        }
        let end = (self.offset + 8192).min(self.secret.expose().len());
        match self.stream.write(&self.secret.expose()[self.offset..end]) {
            Ok(0) => true,
            Ok(n) => {
                self.offset += n;
                self.offset == self.secret.expose().len()
            }
            Err(error) => !matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted),
        }
    }
}

extern "C" {
    fn fcntl(fd: i32, command: i32, ...) -> i32;
}

pub(super) fn nonblocking(stream: &impl AsRawFd) -> io::Result<()> {
    const F_GETFL: i32 = 3;
    const F_SETFL: i32 = 4;
    const O_NONBLOCK: i32 = 0o4000;
    // SAFETY: the borrowed stream holds fd open, and F_GETFL takes no third argument.
    let flags = unsafe { fcntl(stream.as_raw_fd(), F_GETFL) };
    if flags < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fd remains open, and F_SETFL takes the existing flags plus O_NONBLOCK as an int.
    if unsafe { fcntl(stream.as_raw_fd(), F_SETFL, flags | O_NONBLOCK) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
