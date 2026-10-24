use super::{source::Sources, Snapshot};
use async_channel::{Receiver, Sender};
use std::{
    sync::{mpsc, Arc, Mutex},
    thread,
    time::Duration,
};

struct Subscriber {
    sender: Sender<Snapshot>,
    pending: Receiver<Snapshot>,
}

struct State {
    current: Snapshot,
    subscribers: Vec<Subscriber>,
}

#[derive(Clone)]
pub struct Service {
    state: Arc<Mutex<State>>,
    _stop: Arc<mpsc::Sender<()>>,
}

/// The family a launch uses when fc-match cannot answer: Omarchy's Style.qml:512-522 keeps `monospace`.
const FALLBACK_FAMILY: &str = "monospace";

fn log(what: &str, error: impl std::fmt::Display) {
    eprintln!("flea-gui: presentation {what}: {error}");
}

impl Service {
    /// Never refuses a launch: an unreadable input counts as absent and a failed font probe keeps the fallback family, each logged, so the first frame is already complete.
    pub fn start() -> (Self, Snapshot) {
        let sources = Sources::current();
        let mut seen = Some(sources.stamps());
        let (mut inputs, failures) = sources.read_for_launch();
        for failure in failures {
            log("input", failure);
        }
        let (initial, probe) = sources.resolve(&inputs, FALLBACK_FAMILY);
        if let Some(error) = probe {
            log("font probe", error);
        }
        let state = Arc::new(Mutex::new(State {
            current: initial.clone(),
            subscribers: Vec::new(),
        }));
        let worker = state.clone();
        let (stop, stopping) = mpsc::channel();
        let spawned = thread::Builder::new()
            .name("flea-presentation".into())
            .spawn(move || {
                while matches!(
                    stopping.recv_timeout(Duration::from_millis(500)),
                    Err(mpsc::RecvTimeoutError::Timeout)
                ) {
                    // One stat per input; nothing is read and no probe runs while they are unchanged.
                    let stamps = sources.stamps();
                    if Some(stamps) == seen {
                        continue;
                    }
                    if !matches!(
                        stopping.recv_timeout(Duration::from_millis(50)),
                        Err(mpsc::RecvTimeoutError::Timeout)
                    ) {
                        return;
                    }
                    if sources.stamps() != stamps {
                        continue;
                    }
                    // Attempt stamps suppress unchanged failures; accepted bytes move only after a stable resolve.
                    seen = Some(stamps);
                    let changed = match sources.read() {
                        Ok(changed) => changed,
                        Err(error) => {
                            log("input", error);
                            continue;
                        }
                    };
                    if changed == inputs {
                        continue;
                    }
                    let family = worker
                        .lock()
                        .expect("presentation subscribers")
                        .current
                        .fonts
                        .family
                        .clone();
                    let (snapshot, probe) = sources.resolve(&changed, &family);
                    if let Some(error) = probe {
                        log("font probe", error);
                    }
                    if sources.stamps() != stamps {
                        // An input moved while resolving: the next tick sees it as a change and resolves again.
                        seen = None;
                        continue;
                    }
                    inputs = changed;
                    let mut state = worker.lock().expect("presentation subscribers");
                    if !snapshot.parsed.0 && state.current.parsed.0 {
                        continue;
                    }
                    if state.current == snapshot {
                        continue;
                    }
                    state.current = snapshot.clone();
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
            });
        if let Err(error) = spawned {
            log("watcher", error);
        }
        (
            Self {
                state,
                _stop: Arc::new(stop),
            },
            initial,
        )
    }

    pub(crate) fn subscribe(&self) -> Receiver<Snapshot> {
        let (sender, receiver) = async_channel::bounded(1);
        let mut state = self.state.lock().expect("presentation subscribers");
        let _ = sender.try_send(state.current.clone());
        state
            .subscribers
            .retain(|subscriber| subscriber.sender.receiver_count() > 1);
        state.subscribers.push(Subscriber {
            sender,
            pending: receiver.clone(),
        });
        receiver
    }
}
