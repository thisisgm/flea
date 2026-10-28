//! The last flyout keeps its rows and geometry during Reveal.qml's close fade.
use super::{Flyout, Menu};
use std::{
    cell::Cell,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

pub const CLOSE: Duration = Duration::from_millis(140);
static NEXT: AtomicU64 = AtomicU64::new(1);

pub struct Faded {
    pub flyout: Flyout,
    pub id: u64,
    pub started: Option<Instant>,
    pub painted: Rc<Cell<f32>>,
}

impl Menu {
    pub(super) fn retire_flyout(&mut self) {
        if let Some(flyout) = self.flyout.take() {
            self.faded = Some(Faded {
                flyout,
                id: NEXT.fetch_add(1, Ordering::Relaxed),
                started: None,
                painted: Rc::new(Cell::new(1.)),
            });
        }
    }

    pub fn settle_fade(&mut self, now: Instant, reduced_motion: bool) {
        if let Some(faded) = &mut self.faded {
            let started = *faded.started.get_or_insert(now);
            if reduced_motion || now.saturating_duration_since(started) >= CLOSE {
                faded.painted.set(0.);
                self.faded = None;
            }
        }
    }
}
