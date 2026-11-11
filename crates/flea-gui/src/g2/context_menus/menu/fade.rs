//! Reveal.qml's fades on the menu: the card and its flyout fade in over 180 ms and out over 140 ms, a closing card keeps its rows, and the last flyout keeps its rows and geometry during its own close.
use super::{Card, Flyout, Menu};
use crate::reveal::{Frame, Reveal};
use std::{
    cell::Cell,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

pub const CLOSE: Duration = Duration::from_millis(140);
static NEXT: AtomicU64 = AtomicU64::new(1);

pub fn close_easing(progress: f32) -> f32 {
    1. - (1. - progress).powi(3)
}

#[test]
fn close_easing_has_the_cubic_midpoint_and_exact_endpoints() {
    assert_eq!(close_easing(0.), 0.);
    assert_eq!(close_easing(0.5), 0.875);
    assert_eq!(close_easing(1.), 1.);
}

pub struct Faded {
    pub flyout: Flyout,
    pub id: u64,
    pub started: Option<Instant>,
    // The opacity it closes from: a flyout cut inside its own open fades down from where it stood.
    pub from: f32,
    pub painted: Rc<Cell<f32>>,
}

/// ContextMenu.qml gives the menu no lift, so only the opacity of each frame is drawn.
#[derive(Default)]
pub struct Fades {
    pub closing: Option<Card>,
    card: Reveal,
    flyout: Reveal,
    pub(crate) card_frame: Frame,
    pub(crate) flyout_frame: Frame,
}

impl Menu {
    /// A closing card stays drawn, without owning any keyboard or pointer input.
    pub fn visible(&self) -> bool {
        self.opened() || self.fades.closing.is_some()
    }

    /// Samples both fades for this frame and answers whether another frame must follow.
    pub fn sample_fades(&mut self, now: Instant, reduced_motion: bool) -> bool {
        let fades = &mut self.fades;
        fades.card_frame = fades.card.sample(self.card.is_some(), now, reduced_motion);
        fades.flyout_frame = fades
            .flyout
            .sample(self.flyout.is_some(), now, reduced_motion);
        // The sample that ends a closing card asks for one more frame, so its host sees it gone and drops it.
        let ended = !fades.card_frame.visible && fades.closing.take().is_some();
        fades.card_frame.animating || fades.flyout_frame.animating || ended
    }

    /// The card a frame draws, the open one or the one closing, at its opacity.
    pub fn revealed_card(&self) -> Option<(&Card, f32)> {
        self.card
            .as_ref()
            .or(self.fades.closing.as_ref())
            .map(|card| (card, self.fades.card_frame.opacity))
    }

    pub(super) fn retire_flyout(&mut self) {
        let from = self.fades.flyout_frame.opacity;
        // A flyout nobody has seen yet has nothing to fade.
        if let Some(flyout) = self.flyout.take().filter(|_| from > 0.) {
            self.faded = Some(Faded {
                flyout,
                id: NEXT.fetch_add(1, Ordering::Relaxed),
                started: None,
                from,
                painted: Rc::new(Cell::new(from)),
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
