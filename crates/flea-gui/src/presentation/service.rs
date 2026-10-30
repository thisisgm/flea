use super::{source::Sources, Snapshot};
use async_channel::{Receiver, Sender};
use std::{
    sync::{mpsc, Arc, Mutex, PoisonError},
    thread,
    time::Duration,
};

struct Subscriber {
    sender: Sender<Snapshot>,
    pending: Receiver<Snapshot>,
}

struct State {
    current: Snapshot,
    #[cfg(test)]
    published: u64,
    subscribers: Vec<Subscriber>,
}

// Clears the subscribers when the worker ends, so every reader's channel closes instead of waiting on a dead worker.
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

/// The face a launch draws until fc-match answers: gpui finds no face named `monospace` and falls to sans, so this is Omarchy's default.
pub(super) const FALLBACK_FAMILY: &str = "JetBrainsMono Nerd Font";
/// How often the worker stats its inputs; the hung-probe test counts its margin in these.
pub(super) const TICK: Duration = Duration::from_millis(500);

fn log(what: &str, error: impl std::fmt::Display) {
    eprintln!("flea-gui: presentation {what}: {error}");
}

impl Service {
    /// Never refuses a launch: an unreadable input counts as absent and a failed font probe keeps the fallback family, each logged, so the first frame is already complete.
    pub fn start() -> (Self, Snapshot) {
        Self::start_with(Sources::current(), || {})
    }

    // `before_watch` runs on the worker before its first poll; a test passes one that panics.
    pub(super) fn start_with(
        sources: Sources,
        before_watch: impl FnOnce() + Send + 'static,
    ) -> (Self, Snapshot) {
        let mut seen = Some(sources.stamps());
        let (mut inputs, failures) = sources.read_for_launch();
        for failure in failures {
            log("input", failure);
        }
        let (initial, probe) = sources.resolve(&inputs, FALLBACK_FAMILY);
        // A quick probe that missed its deadline earns one patient retry on the worker, and that retry earns none.
        let timed_out = |probe: &Option<std::io::Error>| {
            probe
                .as_ref()
                .is_some_and(|error| error.kind() == std::io::ErrorKind::TimedOut)
        };
        let mut retry = timed_out(&probe);
        if let Some(error) = probe {
            log("font probe", error);
        }
        let state = Arc::new(Mutex::new(State {
            current: initial.clone(),
            #[cfg(test)]
            published: 0,
            subscribers: Vec::new(),
        }));
        let worker = state.clone();
        let (stop, stopping) = mpsc::channel();
        let spawned = thread::Builder::new()
            .name("flea-presentation".into())
            .spawn(move || {
                let _closing = Closing(worker.clone());
                before_watch();
                while matches!(
                    stopping.recv_timeout(TICK),
                    Err(mpsc::RecvTimeoutError::Timeout)
                ) {
                    // One stat per input; nothing is read and no probe runs while they are unchanged.
                    let stamps = sources.stamps();
                    if Some(stamps) == seen && !retry {
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
                    let patient = std::mem::take(&mut retry);
                    let changed = match sources.read() {
                        Ok(changed) => changed,
                        Err(error) => {
                            log("input", error);
                            // The retry re-asks fontconfig for the inputs already held, so a failed read cannot spend it.
                            if !patient {
                                continue;
                            }
                            inputs.clone()
                        }
                    };
                    if changed == inputs && !patient {
                        continue;
                    }
                    let family = worker
                        .lock()
                        .expect("presentation subscribers")
                        .current
                        .fonts
                        .family
                        .clone();
                    let (snapshot, probe) = if patient {
                        sources.resolve_patient(&changed, &family)
                    } else {
                        sources.resolve(&changed, &family)
                    };
                    retry = !patient && timed_out(&probe);
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
                    #[cfg(test)]
                    {
                        state.published += 1;
                    }
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

    #[cfg(test)]
    pub(crate) fn publications(&self) -> u64 {
        self.state
            .lock()
            .expect("presentation subscribers")
            .published
    }

    pub(crate) fn subscribe(&self) -> Receiver<Snapshot> {
        let (sender, receiver) = async_channel::bounded(1);
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
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

#[cfg(test)]
mod tests {
    use super::{Service, Sources};
    use std::{path::PathBuf, sync::mpsc, time::Duration};

    // The window's presentation reader ends when the worker dies, and a later subscribe still answers.
    #[test]
    fn m9_integ_g05_presentation_worker_exit_closes_every_reader() {
        let sources = Sources::at(
            std::array::from_fn(|_| PathBuf::from("/proc/self/flea-m9-absent-input")),
            PathBuf::from("/proc/self/flea-m9-no-cache/entry"),
        );
        let (service, initial) = Service::start_with(sources, || panic!("the watcher dies"));
        let reader = service.subscribe();
        let (report, outcome) = mpsc::channel();
        std::thread::spawn(move || {
            let first = reader.recv_blocking().ok();
            let _ = report.send((first, reader.recv_blocking().is_err()));
        });
        let (first, closed) = outcome
            .recv_timeout(Duration::from_secs(10))
            .expect("the reader blocked on a dead presentation worker");
        assert!(
            first == Some(initial.clone()),
            "the buffered snapshot was lost"
        );
        assert!(closed, "the reader stayed open after the worker died");
        let late = service.subscribe();
        assert!(
            late.try_recv().ok() == Some(initial),
            "a late subscribe lost the current snapshot"
        );
    }
}
