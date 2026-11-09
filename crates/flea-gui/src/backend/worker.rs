use super::{failure, transport, Closing, Control, Event, Failure};
use flea::protocol::Request;
use std::io::{self, Read, Write};
use std::os::{fd::AsRawFd, unix::net::UnixStream};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{mpsc::Receiver, Arc, Mutex, PoisonError};
use std::time::Instant;

#[repr(C)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}

extern "C" {
    fn fcntl(fd: i32, command: i32, ...) -> i32;
    fn poll(fds: *mut PollFd, count: usize, timeout: i32) -> i32;
}

const F_GETFL: i32 = 3;
const F_SETFL: i32 = 4;
const POLLIN: i16 = 1;
const POLLOUT: i16 = 4;

fn nonblocking(pipe: &impl AsRawFd) -> io::Result<()> {
    let flags = unsafe { fcntl(pipe.as_raw_fd(), F_GETFL) };
    if flags < 0
        || unsafe { fcntl(pipe.as_raw_fd(), F_SETFL, flags | flea::oflags::O_NONBLOCK) } < 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

struct Pending {
    bytes: Vec<u8>,
    offset: usize,
    window: bool,
    quit: bool,
}

impl Pending {
    fn from_line(line: String) -> Self {
        let mut bytes = line.into_bytes();
        if !bytes.ends_with(b"\n") {
            bytes.push(b'\n');
        }
        Self {
            bytes,
            offset: 0,
            window: false,
            quit: false,
        }
    }

    fn new(control: Control) -> Self {
        let mut bytes = Vec::new();
        let mut window = false;
        let mut quit = false;
        match control {
            Control::List {
                path,
                select,
                first,
            } => Request::List {
                path: &path,
                select: select.as_deref(),
                first,
            }
            .write_to(&mut bytes)
            .expect("writing a Vec"),
            Control::Window { start, count } => {
                window = true;
                Request::Window { start, count }
                    .write_to(&mut bytes)
                    .expect("writing a Vec")
            }
            Control::Rows { command, action } => {
                transport::write_rows(&mut bytes, command, &action).expect("writing a Vec")
            }
            Control::Line(line) => {
                bytes.extend_from_slice(line.as_bytes());
                bytes.push(b'\n');
            }
            Control::Quit => {
                quit = true;
                Request::Quit.write_to(&mut bytes).expect("writing a Vec")
            }
        }
        Self {
            bytes,
            offset: 0,
            window,
            quit,
        }
    }

    fn write(&mut self, input: &mut ChildStdin) -> io::Result<bool> {
        match input.write(&self.bytes[self.offset..]) {
            Ok(0) => Err(io::ErrorKind::WriteZero.into()),
            Ok(count) => {
                self.offset += count;
                Ok(true)
            }
            Err(error) if retry(&error) => Ok(false),
            Err(error) => Err(error),
        }
    }
}

fn retry(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
    )
}

pub(super) struct Worker<'a> {
    requests: Receiver<Control>,
    wake: UnixStream,
    wake_open: bool,
    closing: Arc<Mutex<Closing>>,
    folders: Arc<Mutex<super::Writable>>,
    transfers: Arc<Mutex<super::transfers::Transfers>>,
    output: &'a async_channel::Sender<Event>,
    bytes: Vec<u8>,
    eof: bool,
}

impl<'a> Worker<'a> {
    pub(super) fn new(
        requests: Receiver<Control>,
        wake: UnixStream,
        closing: Arc<Mutex<Closing>>,
        (folders, transfers): (
            Arc<Mutex<super::Writable>>,
            Arc<Mutex<super::transfers::Transfers>>,
        ),
        output: &'a async_channel::Sender<Event>,
    ) -> Self {
        Self {
            requests,
            wake,
            wake_open: true,
            closing,
            folders,
            transfers,
            output,
            bytes: Vec::new(),
            eof: false,
        }
    }

    pub(super) fn run(mut self, mut command: Command, first_line: String) -> Result<(), Failure> {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| failure("window.backend_start", error))?;
        let input = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let initial = Pending::from_line(first_line);
        let result = self.pump(input, stdout, initial);
        // pump owns both pipes, so stdin closes before a forced kill and no reader outlives reap; after a stdin close the child is only reaped, never killed.
        let released = *self.closing.lock().unwrap() == Closing::Stdin;
        if result.is_err() && !released {
            let _ = child.kill();
        }
        let waited = reap(child, released).map_err(|error| failure("window.backend_wait", error));
        result?;
        waited?;
        Ok(())
    }

    fn pump(
        &mut self,
        input: ChildStdin,
        mut stdout: ChildStdout,
        initial: Pending,
    ) -> Result<(), Failure> {
        nonblocking(&input).map_err(|error| failure("window.backend_write", error))?;
        nonblocking(&stdout).map_err(|error| failure("window.backend_read", error))?;
        let mut input = Some(input);
        let mut pending = Some(initial);
        loop {
            if self.wake_open {
                match self.wake.read(&mut [0; 1024]) {
                    Ok(0) => self.wake_open = false,
                    Ok(_) => (),
                    Err(error) if retry(&error) => (),
                    Err(error) => return Err(failure("window.backend_read", error)),
                }
            }
            // Read close state after consuming its wakeup so poll cannot miss a concurrent quit.
            let closing = *self.closing.lock().unwrap();
            let deadline = match closing {
                Closing::Quit(deadline) => Some(deadline),
                Closing::Open | Closing::Stdin => None,
            };
            if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                return Err(failure("window.backend_timeout", ""));
            }
            // A partly written line must finish before quit or a stdin close, but untouched queued windows are discarded.
            if closing != Closing::Open
                && pending.as_ref().is_some_and(|p| p.window && p.offset == 0)
            {
                pending = None;
            }
            if pending.is_none() && input.is_some() {
                pending = match closing {
                    Closing::Open => self.requests.try_recv().ok().map(Pending::new),
                    Closing::Quit(_) => {
                        // Quit follows the work accepted before it; only queued windows are dropped.
                        let mut chosen = None;
                        while let Ok(control) = self.requests.try_recv() {
                            if matches!(control, Control::Window { .. }) {
                                continue;
                            }
                            chosen = Some(Pending::new(control));
                            break;
                        }
                        Some(chosen.unwrap_or_else(|| Pending::new(Control::Quit)))
                    }
                    Closing::Stdin => {
                        input = None;
                        None
                    }
                };
            }
            let mut progress = false;
            if !self.eof {
                progress = self.read(&mut stdout)?;
            }
            if self.eof {
                match closing {
                    Closing::Open => return Err(failure("window.backend_eof", "")),
                    // No deadline after a stdin close: run blocks in wait until the child leaves on its own.
                    Closing::Stdin => return Ok(()),
                    // The backend lets go of its output only once its shutdown is done, so the end is the drain.
                    Closing::Quit(_) => return Ok(()),
                }
            }
            if let (Some(write), Some(input)) = (&mut pending, &mut input) {
                progress |= write
                    .write(input)
                    .map_err(|error| failure("window.backend_write", error))?;
            }
            if pending.as_ref().is_some_and(|p| p.offset == p.bytes.len()) {
                let sent = pending.take().unwrap();
                if sent.quit {
                    input = None;
                }
            }
            if !progress {
                self.wait(input.as_ref(), &stdout, pending.is_some(), deadline)?;
            }
        }
    }

    fn read(&mut self, stdout: &mut ChildStdout) -> Result<bool, Failure> {
        let mut chunk = [0; 16 * 1024];
        let count = match stdout.read(&mut chunk) {
            Ok(count) => count,
            Err(error) if retry(&error) => return Ok(false),
            Err(error) => return Err(failure("window.backend_read", error)),
        };
        self.eof = count == 0;
        let previous = self.bytes.len();
        self.bytes.extend_from_slice(&chunk[..count]);
        let mut start = 0;
        for end in previous..self.bytes.len() {
            if self.bytes[end] == b'\n' {
                let line = &self.bytes[start..end];
                self.line(line.strip_suffix(b"\r").unwrap_or(line));
                start = end + 1;
            }
        }
        if self.eof && start < self.bytes.len() {
            self.line(&self.bytes[start..]);
            start = self.bytes.len();
        }
        self.bytes.drain(..start);
        Ok(count != 0)
    }

    fn line(&self, bytes: &[u8]) {
        let line = match std::str::from_utf8(bytes) {
            Ok(line) => line,
            Err(error) => {
                let _ = self
                    .output
                    .try_send(Event::Garbled(failure("window.backend_protocol", error)));
                return;
            }
        };
        let event = wire_event(line);
        // The folder fact lands before its listed event, so a menu built on that landing reads it.
        if let Some((folder, writable)) = transport::listed_folder(&event, line) {
            self.folders
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .record(folder, writable);
        }
        self.transfers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .answered(line);
        if let Some(changed) = transport::reload_changed(&event, line) {
            let _ = self.output.try_send(Event::ReloadChanged(changed));
        }
        let _ = self.output.try_send(event);
    }

    fn wait(
        &self,
        input: Option<&ChildStdin>,
        stdout: &ChildStdout,
        writing: bool,
        deadline: Option<Instant>,
    ) -> Result<(), Failure> {
        let timeout = deadline.map_or(-1, |deadline| {
            deadline
                .saturating_duration_since(Instant::now())
                .as_millis()
                .saturating_add(1)
                .min(i32::MAX as u128) as i32
        });
        let timeout = if self.eof {
            timeout.clamp(0, 10)
        } else {
            timeout
        };
        let mut fds = [
            PollFd {
                fd: if self.wake_open {
                    self.wake.as_raw_fd()
                } else {
                    -1
                },
                events: POLLIN,
                revents: 0,
            },
            PollFd {
                fd: if self.eof { -1 } else { stdout.as_raw_fd() },
                events: POLLIN,
                revents: 0,
            },
            PollFd {
                fd: if writing {
                    input.map_or(-1, AsRawFd::as_raw_fd)
                } else {
                    -1
                },
                events: POLLOUT,
                revents: 0,
            },
        ];
        if unsafe { poll(fds.as_mut_ptr(), fds.len(), timeout) } < 0 {
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(failure("window.backend_read", error));
            }
        }
        Ok(())
    }
}

// A child the kernel holds past its output (a thread stuck in a dead mount's read) is reaped off this thread.
fn reap(mut child: Child, released: bool) -> io::Result<()> {
    if released {
        return child.wait().map(drop);
    }
    if child.try_wait()?.is_none() {
        std::thread::Builder::new()
            .name("flea-backend-reap".into())
            .spawn(move || drop(child.wait()))?;
    }
    Ok(())
}

// One reply line as the pane receives it; a line that is not protocol stays one bad line.
pub(super) fn wire_event(line: &str) -> Event {
    match transport::parse(line) {
        Ok(event) => event,
        Err(error) => Event::Garbled(failure("window.backend_protocol", error)),
    }
}
