//! Test-only Backend constructors that capture the control pipe instead of spawning a child.
use super::{transport, Backend, Closing, Control, Event};
use std::os::unix::net::UnixStream;
use std::sync::{atomic::AtomicUsize, mpsc, Arc, Mutex};

#[path = "test_completion.rs"]
mod completion;

impl Backend {
    pub(crate) fn test_output(&self) -> async_channel::Sender<Event> {
        self.plugin_output.clone()
    }

    /// A folder's listed "w", as the reader thread records it from a listed line.
    pub(crate) fn test_writable(&self, folder: &str, writable: bool) {
        self.folders.lock().unwrap().record(folder.into(), writable);
    }

    pub(crate) fn test_reply_line(line: &str) -> Event {
        transport::parse(line).expect("valid fixture wire")
    }

    /// Any wire line, valid or not, mapped as the reader thread maps it.
    pub(crate) fn test_wire_line(line: &str) -> Event {
        super::worker::wire_event(line)
    }

    pub(crate) fn test_channel() -> (Self, impl FnMut() -> Option<(usize, usize)>) {
        let (backend, receiver) = Self::test_controls();
        let receive = move || loop {
            if let Control::Window { start, count } = receiver.try_recv().ok()? {
                return Some((start, count));
            }
        };
        (backend, receive)
    }

    pub(crate) fn test_lines() -> (Self, impl FnMut() -> Option<String>) {
        let (backend, receiver) = Self::test_controls();
        let receive = move || loop {
            if let Control::Line(line) = receiver.try_recv().ok()? {
                return Some(line);
            }
        };
        (backend, receive)
    }

    pub(crate) fn test_window_lines() -> (Self, impl FnMut() -> Option<String>) {
        let (backend, receiver) = Self::test_controls();
        let receive = move || loop {
            match receiver.try_recv().ok()? {
                Control::Window { start, count } => {
                    return Some(format!(
                        r#"{{"c":"window","start":{start},"count":{count}}}"#
                    ))
                }
                Control::Line(line) => return Some(line),
                _ => {}
            }
        };
        (backend, receive)
    }

    pub(super) fn test_controls() -> (Self, mpsc::Receiver<Control>) {
        let (requests, receiver) = mpsc::channel();
        let (wake, _) = UnixStream::pair().unwrap();
        let (plugin_output, replies) = async_channel::unbounded();
        let (_, drained) = async_channel::bounded(1);
        (
            Self {
                plugin_route: Default::default(),
                plugin_output,
                formats_requests: AtomicUsize::new(0),
                requests,
                closing: Arc::new(Mutex::new(Closing::Open)),
                wake,
                replies,
                drained,
                worker: Default::default(),
                thumbs: Default::default(),
                folders: Default::default(),
                transfers: Default::default(),
            },
            receiver,
        )
    }

    pub(crate) fn test_operation_channel() -> (Self, impl FnMut() -> Option<String>) {
        let (requests, receiver) = mpsc::channel();
        let (wake, _) = UnixStream::pair().unwrap();
        let (plugin_output, replies) = async_channel::unbounded();
        let (_, drained) = async_channel::bounded(1);
        let receive = move || match receiver.try_recv().ok()? {
            Control::Line(line) => Some(line),
            _ => None,
        };
        (
            Self {
                plugin_route: Default::default(),
                plugin_output,
                formats_requests: AtomicUsize::new(0),
                requests,
                closing: Arc::new(Mutex::new(Closing::Open)),
                wake,
                replies,
                drained,
                worker: Default::default(),
                thumbs: Default::default(),
                folders: Default::default(),
                transfers: Default::default(),
            },
            receive,
        )
    }

    pub(crate) fn closing_probe(&self) -> impl Fn() -> Closing {
        let closing = self.closing.clone();
        move || *closing.lock().unwrap()
    }
}
