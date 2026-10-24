//! A pane's backend child; all process and pipe work stays off the UI thread.
use crate::pane::requests::RowAction;
use async_channel::Receiver;
use flea::protocol::{Reply, Request};
use std::io::{self, Write};
use std::os::unix::net::UnixStream;
use std::process::Command;
use std::sync::{
    mpsc::{self, Sender},
    Arc, Mutex,
};
use std::thread;
use std::time::{Duration, Instant};
mod transport;
pub use transport::list_line;

pub mod status;
mod worker;

#[derive(Clone, Debug)]
pub struct Failure {
    pub key: &'static str,
    pub detail: String,
}

#[derive(Debug)]
pub enum Event {
    Filesystem(status::Filesystem),
    /// One wire reply belongs to both the footer and the folder-size planner.
    FilesystemDetails {
        facts: status::Filesystem,
        class: String,
    },
    Plugins(String),
    Path(crate::path::Answer),
    Reply(Reply),
    Session {
        generation: usize,
        reply: Reply,
    },
    Refused {
        operation: String,
        path: String,
        message: String,
        mode: u32,
    },
    Failed(Failure),
    Drained,
    Changed {
        path: String,
    },
    Located(Located),
    Paths(Vec<String>),
    Operations(String),
    ListDetails(crate::list::sizes::Reply),
    Peeked(flea::jsondoc::Json),
    TrashLine(String),
    Producer(String),
}

// Both locate forms: the single path with its index, and the batch with its id and matches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located {
    pub directory: String,
    pub path: String,
    pub index: i64,
    pub id: Option<u64>,
    pub transfer: i64,
    pub ok: bool,
    pub matches: Vec<(String, usize)>,
}

// A quit barrier carries the deadline past which the child is killed; a closed stdin never kills.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Closing {
    Open,
    Quit(Instant),
    Stdin,
}

enum Control {
    List {
        path: String,
        select: Option<String>,
        first: usize,
    },
    Window {
        start: usize,
        count: usize,
    },
    Rows {
        command: &'static str,
        action: RowAction,
    },
    Line(String),
    Quit,
}

pub struct Backend {
    requests: Sender<Control>,
    closing: Arc<Mutex<Closing>>,
    wake: UnixStream,
    pub replies: Receiver<Event>,
    drained: Receiver<()>,
}

fn command() -> Command {
    let mut command = Command::new(crate::window::startup::binary());
    command.arg("--backend");
    command
}

fn failure(key: &'static str, detail: impl ToString) -> Failure {
    Failure {
        key,
        detail: detail.to_string(),
    }
}

impl Backend {
    /// Operations use the same ordered backend pipe and shutdown contract.
    pub fn operation(&self, line: String) -> Result<(), Failure> {
        self.line(line)
    }
    /// Menu producer requests share the ordered backend pipe.
    pub fn line(&self, line: String) -> Result<(), Failure> {
        self.send(Control::Line(line))
    }
    /// A Columns-owned request, already encoded by its peek producer.
    pub(crate) fn columns_line(&self, line: String) -> Result<(), Failure> {
        self.send(Control::Line(line))
    }
    /// Only the path producer writes these bounded completion requests.
    pub fn path_line(&self, line: String) -> Result<(), Failure> {
        self.send(Control::Line(line))
    }
    pub fn start(path: String, select: Option<String>, first: usize) -> io::Result<Self> {
        Self::start_command(command(), path, select, first)
    }

    // Injecting the command lets acceptance drive the real pipe, EOF and reaping path.
    pub fn start_listing(listing: &crate::navigation::Listing) -> io::Result<Self> {
        Self::start_with(command(), transport::list_line(listing))
    }

    pub fn start_command(
        command: Command,
        path: String,
        select: Option<String>,
        first: usize,
    ) -> io::Result<Self> {
        let mut line = Vec::new();
        Request::List {
            path: &path,
            first,
            select: select.as_deref(),
        }
        .write_to(&mut line)?;
        let line = String::from_utf8_lossy(&line).trim_end().to_owned();
        Self::start_with(command, line)
    }

    // The first line is written before any control, so the child always lists first.
    pub fn start_with(command: Command, first_line: String) -> io::Result<Self> {
        let (requests, receiver) = mpsc::channel();
        let (wake, listener) = UnixStream::pair()?;
        wake.set_nonblocking(true)?;
        listener.set_nonblocking(true)?;
        let closing = Arc::new(Mutex::new(Closing::Open));
        let state = closing.clone();
        let (output, replies) = async_channel::unbounded();
        let (done, drained) = async_channel::bounded(1);
        thread::Builder::new()
            .name("flea-backend".into())
            .spawn(move || {
                let worker = worker::Worker::new(receiver, listener, state, &output);
                if let Err(error) = worker.run(command, first_line) {
                    let _ = output.try_send(Event::Failed(error));
                }
                let _ = output.try_send(Event::Drained);
                let _ = done.try_send(());
            })?;
        Ok(Self {
            requests,
            closing,
            wake,
            replies,
            drained,
        })
    }

    // Sample input: {"c":"plugins","op":"enable","id":4,"plugin":"mtorvik.checksums"}
    pub fn plugins(&self, line: String) -> Result<(), Failure> {
        let document = flea::jsondoc::parse(&line).map_err(|_| failure("pane.row_request", ""))?;
        if document.get("c").and_then(flea::jsondoc::Json::as_str) != Some("plugins") {
            return Err(failure("pane.row_request", ""));
        }
        self.send(Control::Line(line))
    }

    pub fn window(&self, start: usize, count: usize) -> Result<(), Failure> {
        self.send(Control::Window { start, count })
    }

    /// Trash owns these serialized requests; the existing pipe worker owns their lifetime and I/O.
    pub fn trash_line(&self, line: String) -> Result<(), Failure> {
        let document = flea::jsondoc::parse(&line).map_err(|_| failure("pane.row_request", ""))?;
        if line.contains(['\n', '\r'])
            || !matches!(
                document.get("c").and_then(flea::jsondoc::Json::as_str),
                Some("trashbrowse" | "trashthumb" | "trashthumbcancel")
            )
        {
            return Err(failure("pane.row_request", ""));
        }
        self.send(Control::Line(line))
    }

    pub fn list(&self, path: String, select: Option<String>, first: usize) -> Result<(), Failure> {
        self.send(Control::List {
            path,
            select,
            first,
        })
    }

    pub fn rows(&self, command: &'static str, action: RowAction) -> Result<(), Failure> {
        if !matches!(command, "paths" | "trash" | "dirsize")
            || action.listing == 0
            || action.rows.is_empty()
        {
            return Err(failure("pane.row_request", ""));
        }
        self.send(Control::Rows { command, action })
    }

    // Navigation submits typed requests; paths reuses the captured-listing row writer.
    pub fn command(&self, command: &crate::navigation::Command) -> Result<(), Failure> {
        if let crate::navigation::Command::Paths(action) = command {
            return self.rows("paths", action.clone());
        }
        match transport::command_line(command) {
            Some(line) => self.send(Control::Line(line)),
            None => Ok(()),
        }
    }

    pub(crate) fn list_details(&self, line: &'static str) -> Result<(), Failure> {
        self.send(Control::Line(line.into()))
    }

    fn send(&self, control: Control) -> Result<(), Failure> {
        let closing = self.closing.lock().unwrap();
        if *closing != Closing::Open {
            return Err(failure("window.backend_stopped", ""));
        }
        self.requests
            .send(control)
            .map_err(|_| failure("window.backend_stopped", ""))?;
        drop(closing);
        self.notify();
        Ok(())
    }

    pub fn drained(&self) -> Receiver<()> {
        self.drained.clone()
    }

    pub fn quit(&self) {
        let mut closing = self.closing.lock().unwrap();
        if *closing == Closing::Open {
            *closing = Closing::Quit(Instant::now() + Duration::from_secs(30));
            drop(closing);
            self.notify();
        }
    }

    // Platform quit: the worker closes stdin and reaps without a deadline, so the child's EOF cleanup runs to its end.
    pub fn close_stdin(&self) {
        *self.closing.lock().unwrap() = Closing::Stdin;
        self.notify();
    }

    fn notify(&self) {
        let _ = (&self.wake).write(&[1]);
    }

    #[cfg(test)]
    pub(crate) fn test_reply_line(line: &str) -> Event {
        transport::parse(line).expect("valid fixture wire")
    }

    #[cfg(test)]
    pub(crate) fn test_channel() -> (Self, impl FnMut() -> Option<(usize, usize)>) {
        let (backend, receiver) = Self::test_controls();
        let receive = move || loop {
            if let Control::Window { start, count } = receiver.try_recv().ok()? {
                return Some((start, count));
            }
        };
        (backend, receive)
    }

    #[cfg(test)]
    pub(crate) fn test_lines() -> (Self, impl FnMut() -> Option<String>) {
        let (backend, receiver) = Self::test_controls();
        let receive = move || loop {
            if let Control::Line(line) = receiver.try_recv().ok()? {
                return Some(line);
            }
        };
        (backend, receive)
    }

    #[cfg(test)]
    fn test_controls() -> (Self, mpsc::Receiver<Control>) {
        let (requests, receiver) = mpsc::channel();
        let (wake, _) = UnixStream::pair().unwrap();
        let (_, replies) = async_channel::unbounded();
        let (_, drained) = async_channel::bounded(1);
        (
            Self {
                requests,
                closing: Arc::new(Mutex::new(Closing::Open)),
                wake,
                replies,
                drained,
            },
            receiver,
        )
    }

    #[cfg(test)]
    pub(crate) fn test_operation_channel() -> (Self, impl FnMut() -> Option<String>) {
        let (requests, receiver) = mpsc::channel();
        let (wake, _) = UnixStream::pair().unwrap();
        let (_, replies) = async_channel::unbounded();
        let (_, drained) = async_channel::bounded(1);
        let receive = move || match receiver.try_recv().ok()? {
            Control::Line(line) => Some(line),
            _ => None,
        };
        (
            Self {
                requests,
                closing: Arc::new(Mutex::new(Closing::Open)),
                wake,
                replies,
                drained,
            },
            receive,
        )
    }

    #[cfg(test)]
    pub(crate) fn closing_probe(&self) -> impl Fn() -> Closing {
        let closing = self.closing.clone();
        move || *closing.lock().unwrap()
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        self.quit();
    }
}

#[cfg(test)]
#[path = "backend/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "backend/native31_fixture.rs"]
mod native31_fixture;
