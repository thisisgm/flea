//! The strip's clocks: a notice's time, the drawn error's time, and the wake the caller arms for the next of them.
use super::{Messages, Tick, ERROR_MS, MESSAGE_MS};
use crate::ui_logic_status_feedback::status::notice_shown;

impl Messages {
    /// Restarts the notice's whole time, or stops it while the notice is hidden or carries undo.
    pub(super) fn sync_notice_timer(&mut self, now: u64) {
        let shown = self.shown();
        let slot = self.slot(&shown);
        let visible = notice_shown(&slot);
        self.edges = (slot.transient_is_error, slot.sticky_here, slot.searching);
        self.notice_until = (!self.notice.is_empty() && visible && !self.notice_hint.stands())
            .then_some(now + MESSAGE_MS);
        self.sync_error_timer(now);
    }

    /// Any other change touches the clock only when an edge flips; a cleared notice needs no wake.
    pub(super) fn follow(&mut self, now: u64) {
        let shown = self.shown();
        let slot = self.slot(&shown);
        let edges = (slot.transient_is_error, slot.sticky_here, slot.searching);
        if edges != self.edges {
            self.sync_notice_timer(now);
            return;
        } else if self.notice.is_empty() {
            self.notice_until = None;
        }
        self.sync_error_timer(now);
    }

    /// The drawn error gets its whole time once; a prompt over it stops its clock, and showing it again restarts it.
    fn sync_error_timer(&mut self, now: u64) {
        let head = if self.shown().is_error {
            self.errors.first()
        } else {
            None
        };
        match head {
            Some(head) if self.error_drawn.as_ref() == Some(head) => {}
            Some(head) => {
                self.error_drawn = Some(head.clone());
                self.error_until = Some(now + ERROR_MS);
            }
            None => {
                self.error_drawn = None;
                self.error_until = None;
            }
        }
    }

    /// The next instant a timer must wake the strip, None at rest.
    pub fn next_deadline(&self) -> Option<u64> {
        [self.notice_until, self.error_until, self.arm_until]
            .into_iter()
            .flatten()
            .min()
    }

    pub fn tick(&mut self, now: u64) -> Tick {
        let mut tick = Tick::default();
        if self.notice_until.is_some_and(|until| now >= until) {
            self.clear_notice();
            self.notice_until = None;
            tick.notice_cleared = true;
        }
        if self.error_until.is_some_and(|until| now >= until) {
            if self.shown().is_error {
                self.errors.remove(0);
            }
            self.error_drawn = None;
            self.follow(now);
        }
        if self.arm_until.is_some_and(|until| now >= until) {
            tick.arm_lost = self.lose_arm(now);
        }
        tick
    }
}
