// A test holds one path's directory read or mode stat until it lets go, the way a stalled mount would.
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

// A hold lets go on its own after this, so a call wrongly made on the test's own thread fails instead of hanging.
const BOUND: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stage {
    Read,
    Mode,
}

struct Slot {
    entered: Sender<()>,
    release: Mutex<Receiver<()>>,
    over: AtomicBool,
    bound: Duration,
}

type Table = Mutex<HashMap<(String, Stage), Arc<Slot>>>;

fn table() -> &'static Table {
    static HOLDS: OnceLock<Table> = OnceLock::new();
    HOLDS.get_or_init(|| Mutex::new(HashMap::new()))
}

// The test's side of one hold: it learns when the stage is reached, lets go, and reads whether the call went on.
pub struct Hold {
    key: (String, Stage),
    slot: Arc<Slot>,
    pub entered: Receiver<()>,
    release: Sender<()>,
}

pub fn hold(path: &str, stage: Stage) -> Hold {
    hold_for(path, stage, BOUND)
}

pub fn hold_for(path: &str, stage: Stage, bound: Duration) -> Hold {
    let (entered_tx, entered) = channel();
    let (release, release_rx) = channel();
    let slot = Arc::new(Slot {
        entered: entered_tx,
        release: Mutex::new(release_rx),
        over: AtomicBool::new(false),
        bound,
    });
    let key = (path.to_string(), stage);
    table()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(key.clone(), Arc::clone(&slot));
    Hold {
        key,
        slot,
        entered,
        release,
    }
}

impl Hold {
    pub fn release(&self) {
        let _ = self.release.send(());
    }

    // True once the held call went on, whether the test released it or the bound did.
    pub fn over(&self) -> bool {
        self.slot.over.load(Ordering::SeqCst)
    }
}

impl Drop for Hold {
    fn drop(&mut self) {
        table()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&self.key);
    }
}

// Called by the read and the stat themselves, so a path nobody holds passes straight through.
pub fn held(path: &str, stage: Stage) {
    let slot = table()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&(path.to_string(), stage))
        .cloned();
    if let Some(slot) = slot {
        let _ = slot.entered.send(());
        let _ = slot
            .release
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .recv_timeout(slot.bound);
        slot.over.store(true, Ordering::SeqCst);
    }
}
