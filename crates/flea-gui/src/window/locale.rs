//! One catalog worker per application, publishing immutable snapshots to every window.
use async_channel::{Receiver, Sender};
use flea::i18n::{I18n, Paths};
use std::io;
use std::sync::{mpsc, Arc, Mutex, PoisonError};
use std::thread;
use std::time::Duration;

// How long a catalog read that raced a file replacement waits before reading again.
const SETTLE: Duration = Duration::from_millis(50);

#[derive(Clone)]
pub struct Snapshot {
    pub revision: u64,
    pub catalog: Arc<I18n>,
}

pub(crate) fn labels(locale: &Snapshot) -> super::composition::Labels {
    super::composition::Labels {
        name: locale.catalog.tr("window.column_name").into(),
        size: locale.catalog.tr("window.column_size").into(),
        modified: locale.catalog.tr("window.column_modified").into(),
    }
}

struct State {
    current: Option<Snapshot>,
    subscribers: Vec<Subscriber>,
}

struct Subscriber {
    sender: Sender<Snapshot>,
    pending: Receiver<Snapshot>,
}

// Clears the subscribers when the worker ends, so a receiver waiting on a dead worker's publish closes.
struct Closing(Arc<Mutex<State>>);

impl Drop for Closing {
    fn drop(&mut self) {
        // A poisoned lock still holds the subscribers, so a panic does not keep them open.
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .subscribers
            .clear();
    }
}

#[derive(Clone)]
pub struct Service {
    state: Arc<Mutex<State>>,
    _stop: Arc<mpsc::Sender<()>>,
}

impl Service {
    pub fn start(paths: Paths) -> io::Result<(Self, Receiver<Snapshot>)> {
        Self::start_with(paths, || {})
    }

    // `before_first` runs on the worker before its first load; a test passes one that panics.
    pub(crate) fn start_with(
        paths: Paths,
        before_first: impl FnOnce() + Send + 'static,
    ) -> io::Result<(Self, Receiver<Snapshot>)> {
        let (send, initial) = async_channel::bounded(1);
        let state = Arc::new(Mutex::new(State {
            current: None,
            subscribers: vec![Subscriber {
                sender: send,
                pending: initial.clone(),
            }],
        }));
        let worker_state = state.clone();
        let (stop, stopping) = mpsc::channel();
        thread::Builder::new()
            .name("flea-catalogs".into())
            .spawn(move || {
                let _closing = Closing(worker_state.clone());
                before_first();
                let mut catalog = I18n::load(paths);
                catalog.mark_paragraphs();
                let _ = catalog.locale().format_date(0);
                let mut revision = 1;
                loop {
                    // A language or catalog file replaced mid-read is read again, so no snapshot mixes two versions.
                    while !catalog.sources_unchanged() {
                        match stopping.recv_timeout(SETTLE) {
                            Err(mpsc::RecvTimeoutError::Timeout) => catalog.reload(),
                            _ => return,
                        }
                    }
                    let snapshot = Snapshot {
                        revision,
                        catalog: Arc::new(catalog.clone()),
                    };
                    {
                        let mut state = worker_state.lock().expect("catalog subscribers");
                        state.current = Some(snapshot.clone());
                        state.subscribers.retain(|subscriber| {
                            if subscriber.sender.receiver_count() <= 1 {
                                return false;
                            }
                            if subscriber.sender.is_full() {
                                let _ = subscriber.pending.try_recv();
                            }
                            subscriber.sender.try_send(snapshot.clone()).is_ok()
                        });
                    }
                    loop {
                        match stopping.recv_timeout(Duration::from_millis(500)) {
                            Err(mpsc::RecvTimeoutError::Timeout) => {}
                            _ => return,
                        }
                        if catalog.poll_changed() {
                            break;
                        }
                    }
                    revision += 1;
                }
            })?;
        Ok((
            Self {
                state,
                _stop: Arc::new(stop),
            },
            initial,
        ))
    }

    /// The service and its first snapshot, or Err when the worker could not start or died before publishing.
    pub fn launch(paths: Paths) -> io::Result<(Self, Snapshot)> {
        Self::launch_with(paths, || {})
    }

    pub(crate) fn launch_with(
        paths: Paths,
        before_first: impl FnOnce() + Send + 'static,
    ) -> io::Result<(Self, Snapshot)> {
        let (service, initial) = Self::start_with(paths, before_first)?;
        // A worker that ended before publishing closed the channel; main refuses the launch with exit 2.
        let first = initial
            .recv_blocking()
            .map_err(|_| io::Error::from(io::ErrorKind::BrokenPipe))?;
        Ok((service, first))
    }

    pub fn subscribe(&self) -> Receiver<Snapshot> {
        let (sender, receiver) = async_channel::bounded(1);
        let mut state = self.state.lock().expect("catalog subscribers");
        if let Some(snapshot) = &state.current {
            let _ = sender.try_send(snapshot.clone());
        }
        state
            .subscribers
            .retain(|subscriber| subscriber.sender.receiver_count() > 1);
        state.subscribers.push(Subscriber {
            sender,
            pending: receiver.clone(),
        });
        receiver
    }

    pub fn current(&self) -> Option<Snapshot> {
        self.state
            .lock()
            .expect("catalog subscribers")
            .current
            .clone()
    }
}
