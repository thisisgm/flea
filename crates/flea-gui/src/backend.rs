//! A pane's backend child; all process and pipe work stays off the UI thread.
use crate::pane::requests::RowAction;
use async_channel::Receiver;
use flea::protocol::{Reply, Request};
use std::io::{self, Write};
use std::os::unix::net::UnixStream;
use std::process::Command;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc::{self, Sender},
    Arc, Mutex, PoisonError,
};
use std::thread;
use std::time::{Duration, Instant};
pub(crate) mod plugin_route;
mod transport;
pub use transport::list_line;

pub mod status;
mod transfers;
mod worker;

#[derive(Clone, Debug)]
pub struct Failure {
    pub key: &'static str,
    pub detail: String,
}

#[derive(Debug)]
pub enum Event {
    Filesystem(status::Filesystem),
    /// Optional fsinfo safety facts travel beside the existing size and footer projections.
    FilesystemSystem {
        system: bool,
        reply: Box<Event>,
    },
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
    /// One invalid reply line; the session continues, but a listing in flight fails as v0.3.12's parse refusal.
    Garbled(Failure),
    Drained,
    Changed {
        path: String,
    },
    /// A reload's listed line named the rows it changed; the listed reply follows it.
    ReloadChanged(usize),
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
    plugin_route: std::sync::OnceLock<plugin_route::Route>,
    plugin_output: async_channel::Sender<Event>,
    formats_requests: AtomicUsize,
    requests: Sender<Control>,
    closing: Arc<Mutex<Closing>>,
    wake: UnixStream,
    pub replies: Receiver<Event>,
    drained: Receiver<()>,
    /// Backend.qml thumbRequests: every thumb line this backend was sent.
    thumbs: std::sync::atomic::AtomicUsize,
    /// Each recently listed folder's "w", which Backend.qml keeps as dirWritable.
    folders: Arc<Mutex<Writable>>,
    transfers: Arc<Mutex<transfers::Transfers>>,
}

/// The "w" of the last folders a backend listed, so a listing of another folder never answers for this one.
#[derive(Default)]
pub(crate) struct Writable(Vec<(String, bool)>);

impl Writable {
    // A pane shows one folder; the rest only cover a swap or a stale session answer landing after it.
    const KEPT: usize = 16;

    pub(crate) fn record(&mut self, folder: String, writable: bool) {
        self.0.retain(|(path, _)| *path != folder);
        if self.0.len() == Self::KEPT {
            self.0.remove(0);
        }
        self.0.push((folder, writable));
    }

    /// A folder no kept listed line names reads writable, Backend.qml's default.
    pub(crate) fn of(&self, folder: &str) -> bool {
        self.0
            .iter()
            .find(|(path, _)| path == folder)
            .is_none_or(|(_, writable)| *writable)
    }
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
    /// How many thumb lines this backend accepted, as Backend.qml counts them.
    pub fn thumb_requests(&self) -> usize {
        self.thumbs.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Backend.qml dirWritable for this folder; a folder no listed line has named yet reads writable.
    pub fn dir_writable(&self, path: &str) -> bool {
        self.folders
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .of(path)
    }
    pub(crate) fn formats_requests(&self) -> usize {
        self.formats_requests.load(Ordering::Relaxed)
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
        let folders = Arc::new(Mutex::new(Writable::default()));
        let listed = folders.clone();
        let transfers = Arc::new(Mutex::new(transfers::Transfers::default()));
        let answered = transfers.clone();
        let exited = transfers.clone();
        let (output, replies) = async_channel::unbounded();
        let (done, drained) = async_channel::bounded(1);
        let plugin_output = output.clone();
        thread::Builder::new()
            .name("flea-backend".into())
            .spawn(move || {
                let worker =
                    worker::Worker::new(receiver, listener, state, (listed, answered), &output);
                if let Err(error) = worker.run(command, first_line) {
                    let _ = output.try_send(Event::Failed(error));
                }
                exited.lock().unwrap_or_else(PoisonError::into_inner).stop();
                let _ = output.try_send(Event::Drained);
                let _ = done.try_send(());
            })?;
        Ok(Self {
            plugin_route: Default::default(),
            plugin_output,
            formats_requests: AtomicUsize::new(0),
            requests,
            closing,
            wake,
            replies,
            drained,
            thumbs: Default::default(),
            folders,
            transfers,
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

    /// A release quiesces the pane first: thumbnails and folder sizes stop reading the volume.
    pub fn quiesce(&self) -> Result<(), Failure> {
        self.send(Control::Line("{\"c\":\"thumbcancel\",\"rows\":[]}".into()))?;
        self.send(Control::Line("{\"c\":\"dirsizecancel\"}".into()))
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
        self.place(control).map(drop)
    }

    /// True when a plugin place took the line instead of the backend.
    fn place(&self, control: Control) -> Result<bool, Failure> {
        let closing = self.closing.lock().unwrap();
        if *closing != Closing::Open {
            return Err(failure("window.backend_stopped", ""));
        }
        if let Some(route) = self.plugin_route.get() {
            if route.intercept(&control)? {
                return Ok(true);
            }
        }
        let thumb = matches!(&control, Control::Line(line) if line.starts_with(r#"{"c":"thumb","#));
        self.requests
            .send(control)
            .map_err(|_| failure("window.backend_stopped", ""))?;
        if thumb {
            self.thumbs
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        drop(closing);
        self.notify();
        Ok(false)
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
#[path = "backend/test_support.rs"]
mod test_support;

#[cfg(test)]
#[path = "backend/native31_fixture.rs"]
mod native31_fixture;
