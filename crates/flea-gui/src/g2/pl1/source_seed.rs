//! The launch's lsblk answer and hostname, read on the startup worker beside the settings so DEVICES draws in the first frame.
use crate::g2::pl1::process::{Event, Handle, Job, Runner, ThreadRunner};
use flea::ui_logic_pl1::devices::LSBLK_ARGV;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// lsblk answers in about 5 ms; one that is slower is killed and the rail's own poll lists instead.
const WAIT: Duration = Duration::from_millis(60);

pub struct Seed {
    pub listing: String,
    pub hostname: String,
}

static SEED: Mutex<Option<Seed>> = Mutex::new(None);

/// Dropping the startup worker's probe on any error also kills and reaps its process through the existing runner.
pub struct Probe(Handle);

impl Drop for Probe {
    fn drop(&mut self) {
        self.0.kill();
        // Startup errors can exit the app immediately, so the runner must reap before this owner returns.
        while self.0.events.recv_blocking().is_ok() {}
    }
}

impl Probe {
    fn listing(self, deadline: Instant) -> Option<String> {
        loop {
            match self.0.events.try_recv() {
                Ok(Event::Exited(outcome)) => return (outcome.code == 0).then_some(outcome.stdout),
                Ok(Event::Started) => continue,
                Err(async_channel::TryRecvError::Empty) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(1));
                }
                _ => return None,
            }
        }
    }
}

/// Starts lsblk before the settings load, so its few milliseconds overlap that read.
pub fn start() -> Probe {
    Probe(ThreadRunner.start(Job::plain(LSBLK_ARGV)))
}

/// Waits a bounded time for the started lsblk and keeps a clean answer for the window's rail.
pub fn finish(probe: Probe, started: Instant) {
    if let Some(listing) = probe.listing(started + WAIT) {
        let hostname = std::fs::read_to_string("/etc/hostname").unwrap_or_default();
        *SEED.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) =
            Some(Seed { listing, hostname });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const TEST_WAIT: Duration = Duration::from_secs(5);

    #[test]
    fn rail_seed_early_startup_error_and_deadline_kill_and_reap_real_children() {
        fn startup(probe: Probe, settings: Result<(), ()>) -> Result<(), ()> {
            let _probe = probe;
            settings?;
            Ok(())
        }
        for timed_out in [false, true] {
            let probe = Probe(ThreadRunner.start(Job::plain(&["sleep", "60"])));
            let events = probe.0.events.clone();
            let deadline = Instant::now() + TEST_WAIT;
            while !matches!(events.try_recv(), Ok(Event::Started)) {
                assert!(Instant::now() < deadline, "real child starts");
                std::thread::sleep(Duration::from_millis(1));
            }
            if timed_out {
                assert!(probe.listing(Instant::now()).is_none());
            } else {
                assert!(startup(probe, Err(())).is_err());
            }
            assert!(
                events.is_closed(),
                "runner ended after reaping the real child"
            );
            assert!(
                Instant::now() < deadline,
                "sleep was killed before its normal exit"
            );
        }
    }
}

#[cfg(test)]
#[path = "source_seed_launch_tests.rs"]
mod launch_tests;

/// The first rail mount takes the seed; a later window lists through its own poll.
pub fn take() -> Option<Seed> {
    SEED.lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take()
}
