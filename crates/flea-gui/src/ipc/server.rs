use async_channel::{Receiver, Sender};
use flea::i18n::I18n;
use flea::ipc::{
    endpoint::Endpoint,
    wire::{self, Reply, Request},
};
use std::io::{self, Read};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc, Arc,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const CLIENT_LIMIT: usize = 32;
const CLIENT_TIMEOUT: Duration = Duration::from_secs(2);
// A plugin action waits for its plugin to start and the menu to validate the row; the caller's own bound is five seconds.
const ACTION_TIMEOUT: Duration = Duration::from_secs(4);

pub enum Event {
    Call {
        request: Request,
        answer: mpsc::SyncSender<Answer>,
    },
    Quit,
}

#[derive(Debug)]
pub struct Answer {
    pub reply: Reply,
    pub quit: bool,
}

pub struct Server {
    endpoint: Endpoint,
    stopping: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    pub events: Receiver<Event>,
}

impl Server {
    // Bind before entering the GUI event loop; no filesystem operation runs on a frame.
    pub fn bind(runtime: &Path) -> io::Result<Self> {
        Self::serve(Endpoint::bind(runtime)?)
    }

    // A file chooser's endpoint, under the role default routing skips.
    pub fn bind_picker(runtime: &Path) -> io::Result<Self> {
        Self::serve(Endpoint::bind_picker(runtime)?)
    }

    fn serve(endpoint: Endpoint) -> io::Result<Self> {
        let listener = endpoint.listener.try_clone()?;
        let i18n = Arc::new(I18n::default());
        let stopping = Arc::new(AtomicBool::new(false));
        let stopped = stopping.clone();
        let (requests, events) = async_channel::bounded(CLIENT_LIMIT);
        let worker = thread::Builder::new()
            .name("flea-ipc".into())
            .spawn(move || {
                let clients = Arc::new(AtomicUsize::new(0));
                for stream in listener.incoming() {
                    if stopped.load(Ordering::Acquire) {
                        break;
                    }
                    let Ok(stream) = stream else { continue };
                    if clients.load(Ordering::Acquire) >= CLIENT_LIMIT {
                        continue;
                    }
                    let count = clients.clone();
                    let requests = requests.clone();
                    let i18n = i18n.clone();
                    count.fetch_add(1, Ordering::AcqRel);
                    let guard = ClientGuard(count);
                    let _ =
                        thread::Builder::new()
                            .name("flea-ipc-client".into())
                            .spawn(move || {
                                let _guard = guard;
                                let _ = serve(stream, &requests, &i18n);
                            });
                }
            })?;
        Ok(Self {
            endpoint,
            stopping,
            worker: Some(worker),
            events,
        })
    }

    pub fn path(&self) -> &Path {
        &self.endpoint.path
    }
}

struct ClientGuard(Arc<AtomicUsize>);

impl Drop for ClientGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

fn read_request(stream: &mut UnixStream, i18n: &I18n) -> io::Result<Request> {
    let deadline = Instant::now() + CLIENT_TIMEOUT;
    let mut bytes = Vec::new();
    let mut chunk = [0; 4096];
    loop {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::TimedOut, i18n.tr("ipc.request_timed_out"))
            })?;
        stream.set_read_timeout(Some(remaining))?;
        let count = match stream.read(&mut chunk) {
            Ok(count) => count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        if count == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        let end = chunk[..count].iter().position(|&b| b == b'\n');
        let used = end.map_or(count, |index| index + 1);
        if bytes.len() + used > wire::REQUEST_LIMIT {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                i18n.tr("ipc.line_too_long"),
            ));
        }
        bytes.extend_from_slice(&chunk[..used]);
        if end.is_some() {
            let line = std::str::from_utf8(&bytes)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
            return Request::decode(line)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error));
        }
    }
}

fn serve(mut stream: UnixStream, requests: &Sender<Event>, i18n: &I18n) -> io::Result<()> {
    stream.set_write_timeout(Some(CLIENT_TIMEOUT))?;
    let request = match read_request(&mut stream, i18n) {
        Ok(request) => request,
        Err(error) => {
            return wire::write_line(
                &mut stream,
                &Reply::error(
                    2,
                    i18n.tr_args("ipc.invalid_request", &[&error.to_string()]),
                )
                .encode(),
            )
        }
    };
    if request == Request::Show {
        return wire::write_line(&mut stream, &flea::ipc::registry::show().encode());
    }
    let wait = match &request {
        Request::Call { function, .. } if function == "runAction" => ACTION_TIMEOUT,
        _ => CLIENT_TIMEOUT,
    };
    let (answer, incoming) = mpsc::sync_channel(1);
    if requests.try_send(Event::Call { request, answer }).is_err() {
        return wire::write_line(
            &mut stream,
            &Reply::error(255, i18n.tr("ipc.window_busy")).encode(),
        );
    }
    let answer = incoming
        .recv_timeout(wait)
        .map_err(|error| io::Error::new(io::ErrorKind::TimedOut, error))?;
    let result = wire::write_line(&mut stream, &answer.reply.encode());
    // Quit only after the reply is flushed; a disconnected caller cannot hold shutdown hostage.
    if answer.quit {
        let _ = requests.send_blocking(Event::Quit);
    }
    result
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        let _ = flea::ipc::connect::with_timeout(&self.endpoint.path, Duration::ZERO);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;
