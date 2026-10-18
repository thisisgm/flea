//! One catalog worker per application, publishing immutable snapshots to every window.
use async_channel::{Receiver, Sender};
use flea::i18n::{I18n, Paths};
use std::io;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Clone)]
pub struct Snapshot {
    pub revision: u64,
    pub catalog: Arc<I18n>,
}

struct State {
    current: Option<Snapshot>,
    subscribers: Vec<Subscriber>,
}

struct Subscriber {
    sender: Sender<Snapshot>,
    pending: Receiver<Snapshot>,
}

#[derive(Clone)]
pub struct Service {
    state: Arc<Mutex<State>>,
    _stop: Arc<mpsc::Sender<()>>,
}

impl Service {
    pub fn start(paths: Paths) -> io::Result<(Self, Receiver<Snapshot>)> {
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
                let mut catalog = I18n::load(paths);
                let _ = catalog.locale().format_date(0);
                let mut revision = 1;
                loop {
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
