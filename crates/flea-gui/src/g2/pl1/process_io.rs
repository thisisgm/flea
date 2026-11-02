//! Nonblocking output with bounded polling and a final snapshot of both streams.
use super::input::{nonblocking, Input};
use super::OUTPUT_CAP;
use std::fs::File;
use std::io::{self, ErrorKind, Read};
use std::os::fd::{AsRawFd, OwnedFd};
use std::process::Child;
use std::thread;
use std::time::Duration;

const WAIT: Duration = Duration::from_millis(5);

/// The most one read takes from a pipe.
const CHUNK: usize = 8192;

pub(super) fn pipes(child: &mut Child) -> io::Result<[Pipe; 2]> {
    Ok([
        Pipe::new(child.stdout.take())?,
        Pipe::new(child.stderr.take())?,
    ])
}

/// One output pipe of a job and what it printed, kept up to OUTPUT_CAP.
pub(super) struct Pipe {
    file: Option<File>,
    kept: Vec<u8>,
}

impl Pipe {
    fn new(stream: Option<impl Into<OwnedFd>>) -> io::Result<Self> {
        let file = stream.map(|stream| File::from(stream.into()));
        if let Some(file) = &file {
            nonblocking(file)?;
        }
        Ok(Self {
            file,
            kept: Vec::new(),
        })
    }

    pub(super) fn open(&self) -> bool {
        self.file.is_some()
    }

    /// Reads one bounded chunk; EOF or a permanent read error closes the pipe.
    fn read(&mut self, limit: usize) -> usize {
        let Some(file) = self.file.as_mut() else {
            return 0;
        };
        let mut chunk = [0u8; CHUNK];
        match file.read(&mut chunk[..limit.min(CHUNK)]) {
            Ok(n) if n > 0 => {
                let room = OUTPUT_CAP.saturating_sub(self.kept.len());
                self.kept.extend_from_slice(&chunk[..n.min(room)]);
                n
            }
            Err(error)
                if matches!(error.kind(), ErrorKind::Interrupted | ErrorKind::WouldBlock) =>
            {
                0
            }
            _ => {
                self.file = None;
                0
            }
        }
    }

    fn buffered(&self) -> usize {
        const FIONREAD: std::ffi::c_ulong = 0x541b;
        let Some(file) = &self.file else {
            return 0;
        };
        let mut available = 0i32;
        // SAFETY: the borrowed fd stays open; FIONREAD writes one int into the live buffer.
        let rc = unsafe { ioctl(file.as_raw_fd(), FIONREAD, &mut available as *mut i32) };
        let available = if rc < 0 {
            OUTPUT_CAP
        } else {
            usize::try_from(available).unwrap_or_default()
        };
        available.min(OUTPUT_CAP.saturating_sub(self.kept.len()))
    }

    pub(super) fn into_text(self) -> String {
        String::from_utf8_lossy(&self.kept).into_owned()
    }
}

/// Snapshot both budgets first, so a continuous producer cannot replenish either one.
pub(super) fn drain_buffered(pipes: &mut [Pipe; 2]) {
    let budgets = pipes.each_ref().map(Pipe::buffered);
    for (pipe, mut remaining) in pipes.iter_mut().zip(budgets) {
        while remaining > 0 {
            let read = pipe.read(remaining);
            if read == 0 {
                break;
            }
            remaining -= read;
        }
    }
}

#[repr(C)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}

extern "C" {
    fn poll(fds: *mut PollFd, count: usize, timeout: i32) -> i32;
    fn ioctl(fd: i32, request: std::ffi::c_ulong, ...) -> i32;
}

const POLLIN: i16 = 1;
const POLLOUT: i16 = 4;

/// Wait at most WAIT, then advance stdin and each ready output by one chunk.
pub(super) fn read_ready(pipes: &mut [Pipe; 2], input: &mut Option<Input>) {
    // poll(2) skips a record whose fd is negative, keeping closed pipes in place.
    let [out, err] = pipes.each_ref().map(|pipe| PollFd {
        fd: pipe.file.as_ref().map_or(-1, |file| file.as_raw_fd()),
        events: POLLIN,
        revents: 0,
    });
    let mut fds = [
        out,
        err,
        PollFd {
            fd: input.as_ref().map_or(-1, Input::fd),
            events: POLLOUT,
            revents: 0,
        },
    ];
    // SAFETY: fds is a live array; each open fd stays owned by its pipe for the call.
    let ready = unsafe { poll(fds.as_mut_ptr(), fds.len(), WAIT.as_millis() as i32) };
    if ready < 0 {
        thread::sleep(WAIT);
        return;
    }
    if fds[2].revents != 0 && input.as_mut().is_some_and(Input::write) {
        *input = None;
    }
    for (pipe, fd) in pipes.iter_mut().zip(fds) {
        if fd.revents != 0 {
            pipe.read(CHUNK);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::os::unix::net::{UnixDatagram, UnixStream};

    #[test]
    fn sol_pl1_drain_final_snapshot_keeps_both_already_buffered_streams() {
        let (out, mut out_writer) = UnixStream::pair().unwrap();
        let (err, mut err_writer) = UnixStream::pair().unwrap();
        out_writer.write_all(&vec![b'o'; 32768]).unwrap();
        err_writer.write_all(&vec![b'e'; 32768]).unwrap();
        let mut pipes = [Pipe::new(Some(out)).unwrap(), Pipe::new(Some(err)).unwrap()];
        drain_buffered(&mut pipes);
        let [out, err] = pipes.map(Pipe::into_text);
        assert_eq!(out, "o".repeat(32768));
        assert_eq!(err, "e".repeat(32768));
    }

    #[test]
    fn sol_pl1_drain_final_snapshot_respects_remaining_capture_capacity() {
        let (out, mut writer) = UnixStream::pair().unwrap();
        writer.write_all(b"buffered past the cap").unwrap();
        let mut out = Pipe::new(Some(out)).unwrap();
        out.kept = vec![b'o'; OUTPUT_CAP - 3];
        let err = Pipe::new(None::<OwnedFd>).unwrap();
        let mut pipes = [out, err];
        drain_buffered(&mut pipes);
        assert_eq!(pipes[0].kept.len(), OUTPUT_CAP);
        assert_eq!(&pipes[0].kept[OUTPUT_CAP - 3..], b"buf");
        assert!(pipes[1].kept.is_empty());
        // One read, never read_to_end, since another test's fork can hold this writer open and EOF may never come.
        let mut unread = [0u8; CHUNK];
        let mut rest = pipes[0].file.take().unwrap();
        let read = rest.read(&mut unread).unwrap();
        assert_eq!(&unread[..read], b"fered past the cap");
    }

    #[test]
    fn sol_pl1_drain_final_snapshot_reads_only_what_fionread_reported() {
        // FIONREAD counts all of a stream but only a datagram socket's first datagram, sized to fill one read.
        let (out, writer) = UnixDatagram::pair().unwrap();
        writer.send(&[b'f'; CHUNK]).unwrap();
        writer.send(b"second").unwrap();
        let (err, mut err_writer) = UnixStream::pair().unwrap();
        err_writer.write_all(&[b'e'; 2 * CHUNK]).unwrap();
        let mut pipes = [Pipe::new(Some(out)).unwrap(), Pipe::new(Some(err)).unwrap()];
        drain_buffered(&mut pipes);
        assert_eq!(pipes[0].kept, [b'f'; CHUNK]);
        assert_eq!(pipes[1].kept, [b'e'; 2 * CHUNK]);
    }
}
