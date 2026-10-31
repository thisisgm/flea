use crate::backend::events::Event;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use crate::backend::events::Sender;
use std::sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex};
use std::time::{Duration, Instant};

// Inotify never delivers a remote change, so the open network folder is re-statted here.
pub const POLL_SECS: u64 = 5;
// One poll's classify plus stat waits this long, then the path is skipped so later folders still poll.
const STAT_TIMEOUT_SECS: u64 = 10;
// A skipped path is tried again after this long, so a recovered share resumes without a restart.
const WEDGED_TTL_SECS: u64 = 60;

// Sample input: "network" and "phone" poll, "" and "usb" never do.
pub fn polls(class: &str) -> bool {
    class == "network" || class == "phone"
}

// Sample input: "/media/nas" answers its symlink mtime, a vanished path answers 0.
fn stat_mtime(path: &Path) -> i64 {
    std::fs::symlink_metadata(path).map(|m| std::os::unix::fs::MetadataExt::mtime(&m)).unwrap_or(0)
}

// Sample input: state (open, 5) with a stat of open at 6 sends, with a stat of another path drops.
fn apply(state: &mut (PathBuf, i64), stat_path: &Path, mtime: i64) -> Option<PathBuf> {
    if *stat_path != state.0 {
        return None;
    }
    if state.1 == 0 {
        state.1 = mtime;
        return None;
    }
    if mtime != state.1 {
        state.1 = mtime;
        return Some(state.0.clone());
    }
    None
}

// One mark per dead path with its helper's return flag, so a blocked helper holds its wedge past the TTL.
type Wedged = Arc<Mutex<HashMap<PathBuf, (Instant, Arc<AtomicBool>)>>>;

// Sample input: stub class "network" with stub mtime 7 answers Some(("network", 7)), a stub blocked on a channel answers None past 50 ms.
fn classify_and_stat_under_deadline(
    path: &Path,
    classify: Arc<dyn Fn(&Path) -> &'static str + Send + Sync>,
    stat: Arc<dyn Fn(&Path) -> i64 + Send + Sync>,
    timeout: Duration,
) -> (Option<(&'static str, i64)>, Arc<AtomicBool>) {
    let done = Arc::new(AtomicBool::new(false));
    let back = Arc::clone(&done);
    let (over, rx) = std::sync::mpsc::channel();
    let owned = path.to_path_buf();
    if std::thread::Builder::new().name("flea-watchpoll-stat".into()).spawn(move || {
        let class = classify(&owned);
        // A class no poll wants costs no stat, so a local folder never waits on its own mtime.
        let mtime = if polls(class) { stat(&owned) } else { 0 };
        back.store(true, Ordering::Relaxed);
        let _ = over.send((class, mtime));
    }).is_err() {
        // No helper runs, so the mark this failure may take is already releasable.
        done.store(true, Ordering::Relaxed);
        return (None, done);
    }
    (rx.recv_timeout(timeout).ok(), done)
}

// Sample input: open /live with no wedge stats and baselines, open /dead already wedged stats nothing.
fn poll_once(
    open: &Path,
    state: &Arc<Mutex<(PathBuf, i64)>>,
    wedged: &Wedged,
    classify: &Arc<dyn Fn(&Path) -> &'static str + Send + Sync>,
    stat: &Arc<dyn Fn(&Path) -> i64 + Send + Sync>,
    timeout: Duration,
    tx: &Sender<Event>,
) {
    {
        let mut wedged = wedged.lock().unwrap_or_else(|p| p.into_inner());
        // A helper still out keeps its mark past the TTL, so one dead path holds one thread.
        wedged.retain(|_, (t, done)| t.elapsed() < Duration::from_secs(WEDGED_TTL_SECS) || !done.load(Ordering::Relaxed));
        if wedged.contains_key(open) {
            return;
        }
    }
    // Classify runs beside the stat under the one deadline, so a wedged lookup never parks the thread.
    let (answered, done) = classify_and_stat_under_deadline(open, Arc::clone(classify), Arc::clone(stat), timeout);
    match answered {
        None => {
            wedged.lock().unwrap_or_else(|p| p.into_inner()).insert(open.to_path_buf(), (Instant::now(), done));
        }
        Some((class, mtime)) => {
            if !polls(class) {
                return;
            }
            let send = {
                let mut held = state.lock().unwrap_or_else(|p| p.into_inner());
                apply(&mut held, open, mtime)
            };
            // A live answer clears the mark, so a recovered share resumes without a restart.
            wedged.lock().unwrap_or_else(|p| p.into_inner()).remove(open);
            if send.is_some() {
                let _ = tx.send(Event::PollChanged(open.to_path_buf()));
            }
        }
    }
}

type OpenFolder = Arc<Mutex<(PathBuf, i64)>>;
type Children = Arc<Mutex<HashMap<PathBuf, OpenFolder>>>;

// One round of the thread: the open folder, then each expanded remote child, each behind its own wedge and deadline.
fn poll_round(
    open: &OpenFolder,
    children: &Children,
    wedged: &Wedged,
    classify: &Arc<dyn Fn(&Path) -> &'static str + Send + Sync>,
    stat: &Arc<dyn Fn(&Path) -> i64 + Send + Sync>,
    timeout: Duration,
    tx: &Sender<Event>,
) {
    let path = open.lock().unwrap_or_else(|p| p.into_inner()).0.clone();
    if path.as_os_str().is_empty() {
        return;
    }
    poll_once(&path, open, wedged, classify, stat, timeout, tx);
    let children: Vec<OpenFolder> = children.lock().unwrap_or_else(|p| p.into_inner()).values().cloned().collect();
    for state in children {
        let path = state.lock().unwrap_or_else(|p| p.into_inner()).0.clone();
        poll_once(&path, &state, wedged, classify, stat, timeout, tx);
    }
}

pub struct Poller {
    children: Children,
    state: Arc<Mutex<(PathBuf, i64)>>,
    startup: super::lazy::Lazy<()>,
}

impl Poller {
    pub fn new(tx: Sender<Event>) -> Self {
        Self::with_hooks(tx, super::extclass::classify, stat_mtime)
    }

    // Split so a test names its classify and stat without touching the network or the clock.
    fn with_hooks(
        tx: Sender<Event>,
        classify: impl Fn(&Path) -> &'static str + Send + Sync + 'static,
        stat: impl Fn(&Path) -> i64 + Send + Sync + 'static,
    ) -> Self {
        let state = Arc::new(Mutex::new((PathBuf::new(), 0i64)));
        let children: Children = Arc::default();
        let extra = children.clone();
        let wedged: Wedged = Arc::new(Mutex::new(HashMap::new()));
        let (at, held) = (Arc::clone(&state), Arc::clone(&wedged));
        let classify: Arc<dyn Fn(&Path) -> &'static str + Send + Sync> = Arc::new(classify);
        let stat: Arc<dyn Fn(&Path) -> i64 + Send + Sync> = Arc::new(stat);
        let startup = super::lazy::Lazy::new(move || {
            std::thread::Builder::new().name("flea-watchpoll".into()).spawn(move || {
                loop {
                    std::thread::sleep(Duration::from_secs(POLL_SECS));
                    poll_round(&at, &extra, &held, &classify, &stat, Duration::from_secs(STAT_TIMEOUT_SECS), &tx);
                }
            }).expect("could not start watch poller");
        });
        Self { children, state, startup }
    }

    pub fn set(&self, path: PathBuf) {
        let _ = &*self.startup;
        *self.state.lock().unwrap_or_else(|p| p.into_inner()) = (path, 0);
    }

    pub fn set_children(&self, paths: impl IntoIterator<Item = PathBuf>) {
        let wanted: Vec<PathBuf> = paths.into_iter().collect();
        let mut children = self.children.lock().unwrap_or_else(|p| p.into_inner());
        children.retain(|path, _| wanted.contains(path));
        for path in wanted {
            children.entry(path.clone()).or_insert_with(|| Arc::new(Mutex::new((path, 0))));
        }
    }

    pub fn clear(&self) {
        self.children.lock().unwrap_or_else(|p| p.into_inner()).clear();
        *self.state.lock().unwrap_or_else(|p| p.into_inner()) = (PathBuf::new(), 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A helper that answers is given the poller's own deadline, so a loaded host cannot turn it into a wedge; the wait ends as soon as it answers.
    const ANSWERS: Duration = Duration::from_secs(STAT_TIMEOUT_SECS);
    // A helper the test holds blocked never answers, so this only sets how long the test waits before calling it wedged.
    const MISSES: Duration = Duration::from_millis(50);
    // A gate the test holds is released at its end; this bound only ends a hold the test never released.
    const HOLD: Duration = Duration::from_secs(30);

    // Rounds driven here, never the thread: the open folder is set on the state directly, since set starts the thread.
    #[test]
    fn tree_outline_remote_children_use_the_existing_poll_and_release_on_collapse() {
        use std::sync::atomic::AtomicI64;
        let (tx, rx) = super::super::events::channel();
        let poller = Poller::with_hooks(tx.clone(), |_| "network", |_| 1);
        *poller.state.lock().unwrap() = (PathBuf::from("/root"), 0);
        let path = PathBuf::from("/root/child");
        poller.set_children([path.clone()]);
        let child_mtime = Arc::new(AtomicI64::new(1));
        let class: Arc<dyn Fn(&Path) -> &'static str + Send + Sync> = Arc::new(|_| "network");
        let stat: Arc<dyn Fn(&Path) -> i64 + Send + Sync> = Arc::new({
            let child_mtime = Arc::clone(&child_mtime);
            move |p| if p == Path::new("/root") { 5 } else { child_mtime.load(Ordering::Relaxed) }
        });
        let wedged: Wedged = Arc::default();
        poll_round(&poller.state, &poller.children, &wedged, &class, &stat, ANSWERS, &tx);
        assert!(rx.try_recv().is_err(), "the first round baselines the open folder and the child silently");
        child_mtime.store(2, Ordering::Relaxed);
        poll_round(&poller.state, &poller.children, &wedged, &class, &stat, ANSWERS, &tx);
        assert!(matches!(rx.try_recv(), Ok(Event::PollChanged(found)) if found == path), "the round polls the expanded child");
        assert!(rx.try_recv().is_err(), "and the open folder, which did not move, says nothing");
        poller.set_children([]);
        assert!(poller.children.lock().unwrap().is_empty());
        child_mtime.store(3, Ordering::Relaxed);
        poll_round(&poller.state, &poller.children, &wedged, &class, &stat, ANSWERS, &tx);
        assert!(rx.try_recv().is_err(), "a collapsed child is never polled again");
    }

    #[test]
    fn a_phone_or_network_class_is_polled_and_local_is_not() {
        // Sample input: each class through the gate the thread calls, never a literal against itself.
        assert!(polls("network"), "network polls");
        assert!(polls("phone"), "phone polls");
        assert!(!polls(""), "local never polls");
        assert!(!polls("usb"), "usb never polls");
        assert_eq!(POLL_SECS, 5, "the named interval stays five seconds");
    }

    #[test]
    fn a_set_between_stat_and_apply_never_baselines_the_new_folder() {
        // Sample input: state (new, 0) with a stat of old at 6 drops and keeps (new, 0).
        let mut state = (PathBuf::from("/new"), 0i64);
        let old_mtime = 6i64;
        let send = apply(&mut state, Path::new("/old"), old_mtime);
        assert!(send.is_none(), "a stat for the old folder must not emit for the new one");
        assert_eq!(state, (PathBuf::from("/new"), 0), "and it must not baseline the new folder with the old mtime");
        let first = apply(&mut state, Path::new("/new"), 9);
        assert!(first.is_none(), "the new folder baselines silently");
        assert_eq!(state.1, 9);
        let second = apply(&mut state, Path::new("/new"), 10);
        assert_eq!(second, Some(PathBuf::from("/new")), "its next move emits");
    }

    #[test]
    fn a_blocked_pair_times_out_while_a_fast_one_answers() {
        // Sample input: blocked stub past 50 ms answers None, instant stub answers Some(("network", 7)).
        let (_hold, gate) = std::sync::mpsc::channel::<()>();
        let gate = Arc::new(Mutex::new(gate));
        let network: Arc<dyn Fn(&Path) -> &'static str + Send + Sync> = Arc::new(|_| "network");
        let blocked_stub: Arc<dyn Fn(&Path) -> i64 + Send + Sync> = Arc::new(move |_| {
            gate.lock().unwrap_or_else(|p| p.into_inner()).recv().ok();
            0
        });
        let blocked = classify_and_stat_under_deadline(Path::new("/dead"), Arc::clone(&network), blocked_stub, MISSES);
        assert!(blocked.0.is_none(), "a dead server must time out rather than wedge polling");
        assert!(!blocked.1.load(Ordering::Relaxed), "its helper is still out, so the mark must stay");
        let fast_stub: Arc<dyn Fn(&Path) -> i64 + Send + Sync> = Arc::new(|_| 7);
        let fast = classify_and_stat_under_deadline(Path::new("/live"), network, fast_stub, ANSWERS);
        assert!(fast.0.is_some_and(|(class, mtime)| class == "network" && mtime == 7), "a live folder still answers under the poller's own deadline");
        assert!(fast.1.load(Ordering::Relaxed), "a returned helper releases its mark to the TTL");
    }

    #[test]
    fn a_dead_share_is_skipped_while_a_later_folder_still_polls() {
        // Sample input: dead path wedged, live path open, so only the live stat runs.
        use std::sync::atomic::{AtomicUsize, Ordering};
        let stat_calls = Arc::new(AtomicUsize::new(0));
        let classify_calls = Arc::new(AtomicUsize::new(0));
        let (tx, rx) = crate::backend::events::channel();
        let state = Arc::new(Mutex::new((PathBuf::new(), 0i64)));
        let wedged = Arc::new(Mutex::new(HashMap::new()));
        wedged.lock().unwrap().insert(PathBuf::from("/dead"), (Instant::now(), Arc::new(AtomicBool::new(false))));
        *state.lock().unwrap() = (PathBuf::from("/live"), 0);
        let classify: Arc<dyn Fn(&Path) -> &'static str + Send + Sync> = Arc::new({
            let seen = Arc::clone(&classify_calls);
            move |_| {
                seen.fetch_add(1, Ordering::Relaxed);
                "network"
            }
        });
        let stat: Arc<dyn Fn(&Path) -> i64 + Send + Sync> = Arc::new({
            let seen = Arc::clone(&stat_calls);
            move |p| {
                seen.fetch_add(1, Ordering::Relaxed);
                assert_eq!(p, Path::new("/live"), "only the live path may reach stat");
                11
            }
        });
        poll_once(Path::new("/live"), &state, &wedged, &classify, &stat, ANSWERS, &tx);
        assert_eq!(state.lock().unwrap().1, 11, "the live folder baselines silently");
        assert!(rx.try_recv().is_err(), "a baseline sends nothing");
        assert_eq!(stat_calls.load(Ordering::Relaxed), 1, "the live path stats once");
        // A helper the wedge failed to stop would answer or panic inside this deadline, so the counts below would see it.
        poll_once(Path::new("/dead"), &state, &wedged, &classify, &stat, ANSWERS, &tx);
        assert_eq!(stat_calls.load(Ordering::Relaxed), 1, "the wedged path never reaches stat");
        assert_eq!(classify_calls.load(Ordering::Relaxed), 1, "nor classify");
    }

    #[test]
    fn a_timed_out_stat_wedges_its_path_until_its_helper_returns() {
        // Sample input: stat held for /dead past a 50 ms deadline wedges /dead while /live still polls.
        use std::sync::atomic::{AtomicUsize, Ordering};
        let stat_calls = Arc::new(AtomicUsize::new(0));
        let (tx, _rx) = crate::backend::events::channel();
        let state = Arc::new(Mutex::new((PathBuf::new(), 0i64)));
        let wedged = Arc::new(Mutex::new(HashMap::new()));
        *state.lock().unwrap() = (PathBuf::from("/dead"), 0);
        let network: Arc<dyn Fn(&Path) -> &'static str + Send + Sync> = Arc::new(|_| "network");
        let (entered, inside) = std::sync::mpsc::channel();
        let (release, gate) = std::sync::mpsc::channel::<()>();
        let gate = Mutex::new(gate);
        let stat: Arc<dyn Fn(&Path) -> i64 + Send + Sync> = Arc::new({
            let seen = Arc::clone(&stat_calls);
            move |p| {
                seen.fetch_add(1, Ordering::Relaxed);
                if p == Path::new("/dead") {
                    let _ = entered.send(());
                    // Held until the test ends, bounded so a regression that waits on it fails instead of hanging.
                    let _ = gate.lock().unwrap_or_else(|p| p.into_inner()).recv_timeout(HOLD);
                    0
                } else {
                    22
                }
            }
        });
        poll_once(Path::new("/dead"), &state, &wedged, &network, &stat, MISSES, &tx);
        assert!(wedged.lock().unwrap().contains_key(&PathBuf::from("/dead")), "a timed-out path is wedged");
        inside.recv_timeout(ANSWERS).expect("the timed-out helper reaches its stat");
        // A second helper would reach its held stat inside this deadline, so the count below would see it.
        poll_once(Path::new("/dead"), &state, &wedged, &network, &stat, ANSWERS, &tx);
        assert_eq!(stat_calls.load(Ordering::Relaxed), 1, "a wedged path spawns no second helper");
        *state.lock().unwrap() = (PathBuf::from("/live"), 0);
        poll_once(Path::new("/live"), &state, &wedged, &network, &stat, ANSWERS, &tx);
        assert_eq!(state.lock().unwrap().1, 22, "a later folder still polls past the wedge");
        drop(release);
    }

    #[test]
    fn a_blocked_classify_times_out_without_parking_later_folders() {
        // Sample input: classify held for /dead past a 50 ms deadline, instant for /live.
        use std::sync::atomic::{AtomicUsize, Ordering};
        let stat_calls = Arc::new(AtomicUsize::new(0));
        let (tx, _rx) = crate::backend::events::channel();
        let state = Arc::new(Mutex::new((PathBuf::new(), 0i64)));
        let wedged = Arc::new(Mutex::new(HashMap::new()));
        *state.lock().unwrap() = (PathBuf::from("/dead"), 0);
        let (release, gate) = std::sync::mpsc::channel::<()>();
        let gate = Mutex::new(gate);
        let classify: Arc<dyn Fn(&Path) -> &'static str + Send + Sync> = Arc::new(move |p| {
            if p == Path::new("/dead") {
                // Held until the test ends, so its stat cannot run inside the test; bounded so a regression fails instead of hanging.
                let _ = gate.lock().unwrap_or_else(|p| p.into_inner()).recv_timeout(HOLD);
            }
            "network"
        });
        let stat: Arc<dyn Fn(&Path) -> i64 + Send + Sync> = Arc::new({
            let seen = Arc::clone(&stat_calls);
            move |_| {
                seen.fetch_add(1, Ordering::Relaxed);
                33
            }
        });
        poll_once(Path::new("/dead"), &state, &wedged, &classify, &stat, MISSES, &tx);
        assert!(wedged.lock().unwrap().contains_key(&PathBuf::from("/dead")), "a blocked classify wedges its own path");
        assert_eq!(stat_calls.load(Ordering::Relaxed), 0, "its stat never runs");
        *state.lock().unwrap() = (PathBuf::from("/live"), 0);
        poll_once(Path::new("/live"), &state, &wedged, &classify, &stat, ANSWERS, &tx);
        assert_eq!(stat_calls.load(Ordering::Relaxed), 1, "a later folder still stats");
        assert_eq!(state.lock().unwrap().1, 33, "and baselines");
        drop(release);
    }
}
