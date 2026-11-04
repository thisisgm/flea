//! Unheld progress uses the same monotonic clock and drawn minimum as a listing swap.
use super::{show_left, HOLD_MS, MIN_SHOW_MS};
use std::{cell::Cell, time::Instant};

#[derive(Default)]
pub struct Clock {
    epoch: Cell<Option<Instant>>,
}

impl Clock {
    pub fn at(now: Instant) -> Self {
        Self {
            epoch: Cell::new(Some(now)),
        }
    }

    pub fn elapsed(&self, now: Instant) -> u64 {
        let epoch = self.epoch.get().unwrap_or_else(|| {
            self.epoch.set(Some(now));
            now
        });
        now.saturating_duration_since(epoch).as_millis() as u64
    }
}

pub struct Progress<T> {
    target: T,
    shown: T,
    active: bool,
    since: Option<u64>,
    mark_since: Option<u64>,
    replacing: bool,
}

impl<T: Default> Default for Progress<T> {
    fn default() -> Self {
        Self {
            target: T::default(),
            shown: T::default(),
            active: false,
            since: None,
            mark_since: None,
            replacing: false,
        }
    }
}

impl<T: Clone + PartialEq + Default> Progress<T> {
    pub fn observe(&mut self, target: &T, active: bool, now: u64) {
        self.progress(active, now);
        if self.target != *target {
            self.target.clone_from(target);
        }
    }

    pub fn progress(&mut self, active: bool, now: u64) {
        if active && !self.active {
            self.since = Some(now);
            self.replacing = self.mark_since.is_some();
        }
        self.active = active;
    }

    // A new producer has its own hold even if the previous producer's last frame remains drawn.
    pub fn restart(&mut self, now: u64) {
        self.active = false;
        self.progress(true, now);
    }

    // A user's own toggle of a surface is direct manipulation: it lands in one frame, with no hold before it draws and no minimum once it has.
    pub fn supersede(&mut self) {
        self.since = None;
        self.mark_since = None;
        self.replacing = false;
    }

    pub fn target(&self) -> &T {
        &self.target
    }

    pub fn shown(&self) -> &T {
        &self.shown
    }

    pub fn holding(&self, now: u64) -> bool {
        (!self.active || self.replacing) && show_left(self.mark_since, now) > 0
    }

    pub fn value(&self, now: u64) -> &T {
        if self.holding(now)
            || (self.active
                && (self.mark_since.is_none() || self.replacing)
                && self
                    .since
                    .is_some_and(|since| now < since.saturating_add(HOLD_MS)))
        {
            &self.shown
        } else {
            &self.target
        }
    }

    // The drawn minimum starts in the frame that first includes progress, never when its timer merely expires.
    pub fn commit(&mut self, now: u64) -> &T {
        if self.holding(now) {
            return &self.shown;
        }
        if self.active {
            if (self.mark_since.is_none() || self.replacing)
                && self
                    .since
                    .is_some_and(|since| now < since.saturating_add(HOLD_MS))
            {
                return &self.shown;
            }
            if self.replacing {
                self.mark_since = None;
            }
            self.mark_since.get_or_insert(now);
        } else {
            self.mark_since = None;
            self.since = None;
        }
        self.replacing = false;
        if self.shown != self.target {
            self.shown.clone_from(&self.target);
        }
        &self.shown
    }

    // A folded or covered surface has not drawn these rows; reopening starts their drawn minimum afresh.
    pub fn hide(&mut self) {
        self.shown = T::default();
        self.mark_since = None;
        self.replacing = false;
    }

    pub fn reveal(&mut self, now: u64) {
        if self.active && self.mark_since.is_some() {
            self.mark_since = Some(now);
        }
    }

    pub fn due(&self, now: u64) -> Option<u64> {
        if self.holding(now) {
            return self
                .mark_since
                .map(|since| since.saturating_add(MIN_SHOW_MS));
        }
        if self.active && (self.mark_since.is_none() || self.replacing) {
            return self
                .since
                .map(|since| since.saturating_add(HOLD_MS))
                .filter(|due| *due > now);
        }
        None
    }
}
