//! The five second "gio mount -li" poll the phone rows read; a listing this ends never replaces the last good one.
use super::process::{Job, Outcome};

pub const POLL_MS: u64 = 5_000;
pub const TIMEOUT_MS: u64 = 10_000;

/// What the listing asks of its driver.
#[derive(Debug)]
pub enum Effect {
    Start(Job),
    Kill,
    Listed,
}

#[derive(Clone, Debug, Default)]
pub struct Listing {
    /// The last listing that finished on time.
    pub text: String,
    /// True once the first listing answered, timed out or not.
    pub answered: bool,
    /// Off parks the poll; the last listing stands.
    pub active: bool,
    pub running: bool,
    poll_again: bool,
    timed_out: bool,
    armed: bool,
    next_poll: u64,
    deadline: Option<u64>,
}

impl Listing {
    pub fn new() -> Self {
        Self {
            active: true,
            ..Self::default()
        }
    }

    /// A poll asked for mid-listing runs the moment the listing ends instead of being dropped.
    pub fn poll(&mut self, now: u64) -> Vec<Effect> {
        if !self.active {
            return Vec::new();
        }
        if self.running {
            self.poll_again = true;
            return Vec::new();
        }
        self.timed_out = false;
        self.running = true;
        self.deadline = Some(now + TIMEOUT_MS);
        vec![Effect::Start(Job::c_locale(vec![
            "gio".into(),
            "mount".into(),
            "-li".into(),
        ]))]
    }

    pub fn tick(&mut self, now: u64) -> Vec<Effect> {
        let mut out = Vec::new();
        if self.active && (!self.armed || now >= self.next_poll) {
            self.next_poll = now + POLL_MS;
            out.extend(self.poll(now));
        }
        self.armed = self.active;
        if self.deadline.is_some_and(|at| now >= at) {
            self.deadline = None;
            if self.running {
                self.timed_out = true;
                out.push(Effect::Kill);
            }
        }
        out
    }

    /// The first poll, periodic poll or in-flight timeout, whichever needs the driver next; a running listing waits only on its timeout.
    pub fn due(&self, now: u64) -> Option<u64> {
        let poll =
            (self.active && !self.running).then_some(if self.armed { self.next_poll } else { now });
        poll.into_iter().chain(self.deadline).min()
    }

    pub fn timed_out(&self) -> bool {
        self.timed_out
    }

    pub fn exited(&mut self, outcome: &Outcome, now: u64) -> Vec<Effect> {
        self.running = false;
        self.deadline = None;
        let mut out = Vec::new();
        if !self.timed_out {
            self.text = outcome.stdout.clone();
            out.push(Effect::Listed);
        }
        self.answered = true;
        if std::mem::take(&mut self.poll_again) {
            out.extend(self.poll(now));
        }
        out
    }
}
