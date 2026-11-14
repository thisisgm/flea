use super::{
    source::Sources,
    wake::{self, Signal},
    Snapshot,
};
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
    // False once no worker will publish again, so a later subscribe gets the current snapshot and a closed channel.
    open: bool,
}

impl State {
    fn close(&mut self) {
        self.open = false;
        self.subscribers.clear();
    }
}

// Clears the subscribers when the worker ends, so every reader's channel closes instead of waiting on a dead worker.
struct Closing(Arc<Mutex<State>>);

impl Drop for Closing {
    fn drop(&mut self) {
        // A poisoned lock still holds the subscribers, so a panic does not keep them open.
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .close();
    }
}

// Dropped with the last Service clone, so the worker stops at once though the waker still holds a sender.
struct Stop(mpsc::Sender<Signal>);

impl Drop for Stop {
    fn drop(&mut self) {
        let _ = self.0.send(Signal::Stop);
    }
}

#[derive(Clone)]
pub struct Service {
    state: Arc<Mutex<State>>,
    _stop: Arc<Stop>,
}

/// The face a launch draws until fc-match answers: gpui finds no face named `monospace` and falls to sans, so this is Omarchy's default.
pub(super) const FALLBACK_FAMILY: &str = "JetBrainsMono Nerd Font";
/// How often the worker stats its inputs when no folder event woke it; the hung-probe test counts its margin in these.
pub(super) const TICK: Duration = Duration::from_millis(500);

// How long inputs must hold still before they are read; Omarchy's swap and an editor's save each land well inside it.
const SETTLE: Duration = Duration::from_millis(20);

/// The longest a timed-out probe's patient retry may take to publish: one tick, the settle and the retry's own deadline.
#[cfg(test)]
pub(crate) const RETRY_PUBLISH: Duration = TICK
    .saturating_add(SETTLE)
    .saturating_add(super::face::PATIENT);

fn log(what: &str, error: impl std::fmt::Display) {
    eprintln!("flea-gui: presentation {what}: {error}");
}

// Waits up to `pause`, or only until a wake when `woken` ends it; false once the service stopped.
fn waited(signals: &mpsc::Receiver<Signal>, pause: Duration, woken: bool) -> bool {
    let until = std::time::Instant::now() + pause;
    loop {
        match signals.recv_timeout(until.saturating_duration_since(std::time::Instant::now())) {
            Ok(Signal::Wake) if woken => return true,
            Ok(Signal::Wake) => {}
            Err(mpsc::RecvTimeoutError::Timeout) => return true,
            Ok(Signal::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => return false,
        }
    }
}

impl Service {
    /// Never refuses a launch: an unreadable input counts as absent and a failed font probe keeps the fallback family, each logged, so the first frame is already complete.
    pub fn start() -> (Self, Snapshot) {
        Self::start_with(Sources::current(), TICK, || {})
    }

    // `before_watch` runs on the worker before its first poll; a test passes one that panics.
    pub(super) fn start_with(
        sources: Sources,
        tick: Duration,
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
            open: true,
        }));
        let worker = state.clone();
        let (stop, stopping) = mpsc::channel();
        wake::start(sources.dirs(), tick, stop.clone());
        let spawned = thread::Builder::new()
            .name("flea-presentation".into())
            .spawn(move || {
                let _closing = Closing(worker.clone());
                before_watch();
                // Set when an input moved during the settle, so it is stat'ed again without waiting for a wake.
                let mut again = false;
                loop {
                    if !std::mem::take(&mut again) && !waited(&stopping, tick, true) {
                        return;
                    }
                    // One stat per input; nothing is read and no probe runs while they are unchanged.
                    let stamps = sources.stamps();
                    if Some(stamps) == seen && !retry {
                        continue;
                    }
                    if !waited(&stopping, SETTLE, false) {
                        return;
                    }
                    if sources.stamps() != stamps {
                        again = true;
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
            state.lock().unwrap_or_else(PoisonError::into_inner).close();
        }
        (
            Self {
                state,
                _stop: Arc::new(Stop(stop)),
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
        if !state.open {
            return receiver;
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
}

#[cfg(test)]
mod tests {
    use super::{Service, Sources};
    use std::{path::PathBuf, sync::mpsc, time::Duration};

    // A tick no test run reaches, so only a folder event can make the worker publish.
    const UNREACHED_TICK: Duration = Duration::from_secs(3600);
    // Turns a swap the worker never saw into a failure instead of a hang.
    const GUARD: Duration = Duration::from_secs(10);

    // Omarchy swaps the whole theme folder; its wake alone publishes the swapped-in theme, never the gap (THEME-GLASS, native2 GPUI 475 ms, v0.3.12 27 ms).
    #[test]
    fn theme_glass_a_theme_folder_swap_publishes_the_swapped_in_theme() {
        let fixture = crate::presentation::fixture::Fixture::new("swap");
        let dir = fixture.path().to_path_buf();
        let theme = dir.join("current/theme");
        let colors =
            |background: &str| format!("background = \"{background}\"\nforeground = \"#c0c0c0\"\n");
        fixture.write(&theme.join("colors.toml"), colors("#101010"));
        let inputs = [
            theme.join("colors.toml"),
            theme.join("shell.toml"),
            dir.join("config/shell.toml"),
            dir.join("fontconfig/fonts.conf"),
            dir.join("fontconfig/conf.d/im.conf"),
            dir.join("toggles/window-opaque.lua"),
            dir.join("toggles/window-glassy.lua"),
            dir.join("gtk-4.0/settings.ini"),
        ];
        let (service, _) = Service::start_with(
            Sources::at(inputs, dir.join("fc-match")),
            UNREACHED_TICK,
            || {},
        );
        let reader = service.subscribe();
        let (forward, published) = mpsc::channel();
        std::thread::spawn(move || {
            while let Ok(snapshot) = reader.recv_blocking() {
                if forward.send(snapshot.theme.background).is_err() {
                    return;
                }
            }
        });
        published
            .recv_timeout(GUARD)
            .expect("the reader got no first snapshot");
        for (round, background) in ["#202020", "#303030", "#404040", "#505050"]
            .into_iter()
            .enumerate()
        {
            let next = dir.join(format!("next-{round}"));
            fixture.write(&next.join("colors.toml"), colors(background));
            fixture.check(&theme).unwrap();
            std::fs::rename(&theme, dir.join(format!("old-{round}"))).unwrap();
            std::fs::rename(&next, &theme).unwrap();
            let want = crate::palette::Theme::from_text_observed(&colors(background))
                .0
                .background;
            assert_eq!(
                published
                    .recv_timeout(GUARD)
                    .expect("a theme folder swap published nothing without a stat tick"),
                want,
                "swap {round} published something other than {background}"
            );
        }
        drop(service);
        fixture.cleanup();
    }

    // The window's presentation reader ends when the worker dies, and a later subscribe still answers.
    #[test]
    fn m9_integ_g05_presentation_worker_exit_closes_every_reader() {
        let sources = Sources::at(
            std::array::from_fn(|_| PathBuf::from("/proc/self/flea-m9-absent-input")),
            PathBuf::from("/proc/self/flea-m9-no-cache/entry"),
        );
        let (service, initial) =
            Service::start_with(sources, super::TICK, || panic!("the watcher dies"));
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

    // A reader that subscribes after the worker died gets the current snapshot and a closed channel, never a wait.
    #[test]
    fn m9_integ_g05_a_subscribe_after_the_worker_exit_still_closes() {
        let sources = Sources::at(
            std::array::from_fn(|_| PathBuf::from("/proc/self/flea-m9-absent-input")),
            PathBuf::from("/proc/self/flea-m9-no-cache/entry"),
        );
        let (die, dying) = mpsc::channel::<()>();
        let (service, initial) = Service::start_with(sources, super::TICK, move || {
            let _ = dying.recv();
            panic!("the watcher dies")
        });
        let closes = |reader: async_channel::Receiver<super::Snapshot>| {
            let (report, outcome) = mpsc::channel();
            std::thread::spawn(move || {
                let first = reader.recv_blocking().ok();
                let _ = report.send((first, reader.recv_blocking().is_err()));
            });
            outcome.recv_timeout(Duration::from_secs(10))
        };
        let before = service.subscribe();
        die.send(()).unwrap();
        // The earlier reader closing proves the worker's exit ran before the next subscribe.
        assert_eq!(
            closes(before).map(|(first, closed)| (first == Some(initial.clone()), closed)),
            Ok((true, true))
        );
        let (first, closed) = closes(service.subscribe())
            .expect("a reader that subscribed after the worker died waited forever");
        assert!(
            first == Some(initial),
            "the late reader lost the current snapshot"
        );
        assert!(closed, "the late reader stayed open after the worker died");
    }
}
