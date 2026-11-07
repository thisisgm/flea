//! The guard that ends a hung case's process, and disarms when the case ends.
use std::sync::mpsc;
use std::time::{Duration, Instant};

// A hang past the deadline ends the process, so it never holds the broker's lock; dropping the guard disarms it.
#[must_use = "the watchdog disarms when this guard drops"]
pub(super) struct Watchdog {
    disarm: mpsc::Sender<()>,
    sleeper: Option<std::thread::JoinHandle<()>>,
}

pub(super) fn watchdog(secs: u64) -> Watchdog {
    let (disarm, armed) = mpsc::channel();
    let sleeper = std::thread::spawn(move || {
        if armed.recv_timeout(Duration::from_secs(secs)) == Err(mpsc::RecvTimeoutError::Timeout) {
            eprintln!("svg test watchdog: {secs} s passed, exiting 97");
            std::process::exit(97);
        }
    });
    Watchdog {
        disarm,
        sleeper: Some(sleeper),
    }
}

impl Drop for Watchdog {
    fn drop(&mut self) {
        let _ = self.disarm.send(());
        if let Some(sleeper) = self.sleeper.take() {
            let _ = sleeper.join();
        }
    }
}

#[test]
fn a_watchdog_disarms_when_its_case_ends() {
    let armed = Instant::now();
    drop(watchdog(1));
    assert!(
        armed.elapsed() < Duration::from_millis(500),
        "disarming waited for the deadline"
    );
    // A sleeper that outlived its guard would end this process with 97 before the sleep returns.
    std::thread::sleep(Duration::from_millis(1500));
}
