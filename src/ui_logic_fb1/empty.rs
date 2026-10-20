//! The empty folder's hero: three phrase beats per appearance, then rest on the third, all on a caller's millisecond clock.
use super::{out_cubic, CLOSE_MS, OPEN_MS, TRANSLATE_UP_PX};

/// The rotating phrases in order; the index carries over from one appearance to the next.
pub const PHRASES: [&str; 7] = [
    "feedback.empty.phrase_1",
    "feedback.empty.phrase_2",
    "feedback.empty.phrase_3",
    "feedback.empty.phrase_4",
    "feedback.empty.phrase_5",
    "feedback.empty.phrase_6",
    "feedback.empty.phrase_7",
];
/// The OEM phrase timer's interval.
pub const ROTATE_MS: u64 = 2800;
/// Beats per appearance before the hero rests.
pub const BEAT_LIMIT: usize = 3;
/// The mark's draw: a hold before the stroke, then the stroke itself.
pub const DRAW_DELAY_MS: u64 = 200;
pub const DRAW_MS: u64 = 1800;

/// What one frame of the hero draws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub phrase: usize,
    pub opacity: f32,
    pub offset: f32,
    pub caption: f32,
    pub mark_opacity: f32,
    /// How much of the spiral's length the mark has drawn, 0 to 1.
    pub mark_drawn: f32,
}

#[derive(Clone, Debug, Default)]
pub struct Rotation {
    index: usize,
    beats: usize,
    visible: bool,
    reduced: bool,
    shown_at: u64,
    entrance_at: Option<u64>,
    fade_at: Option<u64>,
    swapped: bool,
    landed: bool,
    mark_at: Option<u64>,
}

fn progress(now: u64, from: u64, length: u64) -> f32 {
    out_cubic(((now.saturating_sub(from)) as f32 / length as f32).min(1.))
}

impl Rotation {
    pub fn index(&self) -> usize {
        self.index
    }
    pub fn beats(&self) -> usize {
        self.beats
    }
    pub fn visible(&self) -> bool {
        self.visible
    }
    /// Whether a beat's swap has landed in this appearance, at the instant the caption is out and the mark undrawn.
    pub fn landed(&self) -> bool {
        self.landed
    }

    /// An appearance: beats start over and the entrance rises in with the mark replaying; a held landing draws both whole.
    pub fn show(&mut self, now: u64, animate_entrance: bool) {
        self.visible = true;
        self.beats = 0;
        self.landed = false;
        self.shown_at = now;
        self.entrance_at = (animate_entrance && !self.reduced).then_some(now);
        if animate_entrance {
            self.replay(now);
        } else {
            self.mark_at = None;
        }
    }

    /// The caller cuts the hero straight away; a fade half done still lands its swap, as the QML sequence did.
    pub fn hide(&mut self) {
        if self.fade_at.is_some() && !self.swapped {
            self.index = (self.index + 1) % PHRASES.len();
        }
        self.visible = false;
        self.beats = 0;
        self.entrance_at = None;
        self.fade_at = None;
        self.mark_at = None;
    }

    /// Reduced motion stops every animation where it stands, a fade before its swap included, and draws the hero whole.
    pub fn set_reduced(&mut self, reduced: bool, now: u64) {
        self.reduced = reduced;
        if reduced {
            self.entrance_at = None;
            self.fade_at = None;
            self.mark_at = None;
        } else if self.visible {
            // The beat timer restarts one interval from now; beats already spent stay spent.
            self.shown_at = now.saturating_sub(ROTATE_MS * self.beats as u64);
        }
    }

    fn replay(&mut self, now: u64) {
        self.mark_at = (!self.reduced).then_some(now);
    }

    fn next_beat(&self) -> Option<u64> {
        (self.visible && !self.reduced && self.beats < BEAT_LIMIT)
            .then(|| self.shown_at + ROTATE_MS * (self.beats as u64 + 1))
    }

    /// Applies every beat and swap due by `now`, in order.
    pub fn advance(&mut self, now: u64) {
        loop {
            let swap = self
                .fade_at
                .filter(|_| !self.swapped)
                .map(|at| at + CLOSE_MS);
            let beat = self.next_beat();
            match (swap, beat) {
                (Some(at), b) if at <= now && b.map_or(true, |b| at <= b) => {
                    self.index = (self.index + 1) % PHRASES.len();
                    self.swapped = true;
                    self.landed = true;
                }
                (_, Some(at)) if at <= now => {
                    self.beats += 1;
                    self.fade_at = Some(at);
                    self.swapped = false;
                    self.replay(at);
                }
                _ => break,
            }
        }
        if self
            .fade_at
            .is_some_and(|at| now >= at + CLOSE_MS + OPEN_MS)
        {
            self.fade_at = None;
        }
        if self.entrance_at.is_some_and(|at| now >= at + OPEN_MS) {
            self.entrance_at = None;
        }
        if self
            .mark_at
            .is_some_and(|at| now >= at + CLOSE_MS + DRAW_DELAY_MS + DRAW_MS)
        {
            self.mark_at = None;
        }
    }

    pub fn frame(&self, now: u64) -> Frame {
        let entrance = self.entrance_at.map_or(1., |at| progress(now, at, OPEN_MS));
        let caption = match self.fade_at {
            Some(at) if now < at + CLOSE_MS => 1. - progress(now, at, CLOSE_MS),
            Some(at) => progress(now, at + CLOSE_MS, OPEN_MS),
            None => 1.,
        };
        let (mark_opacity, mark_drawn) = match self.mark_at {
            Some(at) if now < at + CLOSE_MS => (1. - progress(now, at, CLOSE_MS), 1.),
            Some(at) => (1., progress(now, at + CLOSE_MS + DRAW_DELAY_MS, DRAW_MS)),
            None => (1., 1.),
        };
        Frame {
            phrase: self.index,
            opacity: entrance,
            offset: TRANSLATE_UP_PX * (1. - entrance),
            caption,
            mark_opacity,
            mark_drawn,
        }
    }

    pub fn mark_settled(&self) -> bool {
        self.mark_at.is_none()
    }

    /// Whether a frame loop is needed now; between beats the hero waits on a timer instead.
    pub fn animating(&self) -> bool {
        self.visible
            && (self.entrance_at.is_some() || self.fade_at.is_some() || self.mark_at.is_some())
    }

    /// The third beat drawn whole: no fade, the entrance done and the mark settled; reduced motion is always at rest.
    pub fn at_rest(&self) -> bool {
        self.reduced || (!self.animating() && self.beats >= BEAT_LIMIT)
    }

    /// When the next beat lands, for a timer; None once the hero rests or while hidden.
    pub fn next_deadline(&self) -> Option<u64> {
        self.next_beat()
    }
}
