//! Shared drag state retains VF3, wide paths and captured flock ownership.
use super::*;

/// Each window's live lift and router registration, held beside the Shell rather than in it.
#[derive(Default)]
pub struct Drags {
    pub(super) lifts: HashMap<u64, Lift>,
    pub(super) clients: HashMap<u64, Registration>,
    /// The marker tag and serial of each window's current lift, shared with its file client.
    pub(super) held: HashMap<u64, Held>,
    pub(super) serial: u64,
    pub(super) flocks: HashMap<u64, gpui::Entity<Flock>>,
    pub(super) said: HashMap<u64, String>,
    /// The rail row each window's held drag rests on.
    pub(super) aims: HashMap<u64, rail_drop::Aim>,
    pub(super) rests: HashMap<u64, spring::Rest>,
    pub(super) hovers: HashMap<u64, Rc<std::cell::RefCell<Option<Arrived>>>>,
    pub(super) cue: Option<(u64, feedback::Cue)>,
    pub(super) cue_lines: std::collections::HashSet<u64>,
    pub(super) asks: HashMap<(u64, usize), wide::Ask>,
    pub(super) waiting: HashMap<(u64, usize), wide::Waiting>,
}

impl Drags {
    pub(super) fn put(&mut self, key: u64, lift: Lift) {
        *self.held.entry(key).or_default().borrow_mut() =
            Some((lift.marker.instance.clone(), lift.serial));
        self.lifts.insert(key, lift);
    }

    pub(super) fn take(&mut self, key: u64) -> Option<Lift> {
        if let Some(held) = self.held.get(&key) {
            *held.borrow_mut() = None;
        }
        self.flocks.remove(&key);
        self.lifts.remove(&key)
    }
}

impl Global for Drags {}
