//! The process's drag state beside every Shell: each window's lift, router registration and the aims its held drag rests on.
use super::super::files::{Held, Hover};
use super::super::router::Registration;
use super::super::Lift;
use gpui::Global;
use std::collections::HashMap;

/// Each window's live lift and router registration, held beside the Shell rather than in it.
#[derive(Default)]
pub struct Drags {
    pub(super) lifts: HashMap<u64, Lift>,
    pub(super) clients: HashMap<u64, Registration>,
    /// The marker tag and serial of each window's current lift, shared with its file client.
    pub(super) held: HashMap<u64, Held>,
    pub(super) serial: u64,
    /// The rail row each window's held drag rests on.
    pub(super) aims: HashMap<u64, super::rail_drop::Aim>,
    /// What another window's drag offered on entering each window.
    pub(super) hovers: HashMap<u64, Hover>,
    /// The folder row, or the bare ground of this window's own lift, each window's held drag rests on.
    pub(super) rows: HashMap<u64, super::row_drop::RowAim>,
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
        self.lifts.remove(&key)
    }
}

impl Global for Drags {}
