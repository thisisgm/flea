//! Shared card reveal from Reveal.qml: cubic fade, an opening rise, and no translation on close.
use flea::ui_logic_status_feedback::{out_cubic, CLOSE_MS, OPEN_MS, TRANSLATE_UP_PX};
use std::time::Instant;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Frame {
    pub opacity: f32,
    pub rise: f32,
    pub visible: bool,
    pub animating: bool,
}

struct Transition {
    at: Instant,
    opacity: f32,
    rise: f32,
    opening: bool,
}

#[derive(Default)]
pub(crate) struct Reveal {
    shown: bool,
    frame: Frame,
    transition: Option<Transition>,
}

impl Reveal {
    pub fn visible(&self) -> bool {
        self.shown || self.frame.opacity > 0. || self.transition.is_some()
    }

    /// The frame the last sample drew.
    #[cfg(test)]
    pub fn drawn(&self) -> Frame {
        self.frame
    }

    fn at(&self, now: Instant) -> Frame {
        let Some(t) = &self.transition else {
            return self.frame;
        };
        let duration = if t.opening { OPEN_MS } else { CLOSE_MS };
        let progress = now.saturating_duration_since(t.at).as_secs_f32() * 1000. / duration as f32;
        let eased = out_cubic(progress);
        let opacity = t.opacity + (if t.opening { 1. } else { 0. } - t.opacity) * eased;
        let rise = if t.opening {
            t.rise * (1. - eased)
        } else if progress >= 1. {
            0.
        } else {
            t.rise
        };
        Frame {
            opacity,
            rise,
            visible: self.shown || opacity > 0.,
            animating: progress < 1.,
        }
    }

    pub fn sample(&mut self, shown: bool, now: Instant, reduced: bool) -> Frame {
        let prior = self.at(now);
        if reduced {
            self.shown = shown;
            self.transition = None;
            self.frame = Frame {
                opacity: if shown { 1. } else { 0. },
                rise: 0.,
                visible: shown,
                animating: false,
            };
            return self.frame;
        }
        if self.shown != shown {
            self.shown = shown;
            self.transition = Some(Transition {
                at: now,
                opacity: prior.opacity,
                rise: if shown && prior.opacity == 0. {
                    TRANSLATE_UP_PX
                } else {
                    prior.rise
                },
                opening: shown,
            });
        }
        self.frame = self.at(now);
        if !self.frame.animating {
            self.transition = None;
        }
        self.frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn pb_network_2_close_freezes_a_partial_rise_and_reopen_continues_it() {
        let now = Instant::now();
        let mut reveal = Reveal::default();
        assert_eq!(reveal.sample(true, now, false).rise, 10.);
        let at = now + Duration::from_millis(90);
        let middle = reveal.sample(true, at, false);
        assert_eq!((middle.opacity, middle.rise), (0.875, 1.25));
        assert_eq!(reveal.sample(false, at, false).rise, middle.rise);
        let half_close = at + Duration::from_millis(70);
        let closing = reveal.sample(false, half_close, false);
        assert_eq!(closing.rise, middle.rise);
        let reopened = reveal.sample(true, half_close, false);
        assert_eq!(
            (reopened.opacity, reopened.rise),
            (closing.opacity, closing.rise)
        );
        let rested = reveal.sample(true, half_close + Duration::from_millis(180), false);
        assert_eq!(
            (rested.opacity, rested.rise, rested.animating),
            (1., 0., false)
        );
    }

    #[test]
    fn pb_network_2_reduced_motion_snaps_both_directions() {
        let now = Instant::now();
        let mut reveal = Reveal::default();
        let open = reveal.sample(true, now, true);
        assert_eq!((open.opacity, open.rise, open.animating), (1., 0., false));
        let close = reveal.sample(false, now, true);
        assert_eq!((close.opacity, close.rise, close.visible), (0., 0., false));
        assert!(!reveal.visible());
    }
}
