//! An arm's prompt: shown only on its owner's live stamp, and ended for the bar and the owner at once.
use super::Messages;
use crate::ui_logic_status_feedback::status::{Arm, Hint};

impl Messages {
    /// An arm's prompt on its own channel; one said before its owner stamped can never end, so it is only a notice.
    pub fn say_arm(&mut self, text: &str, arm_ms: u64, now: u64) {
        if text.is_empty() {
            self.dismiss(now);
            return;
        }
        if self.owner_stamp == 0 {
            self.notice = text.into();
            self.notice_hint = Hint::None;
            self.sync_notice_timer(now);
            return;
        }
        self.clear_notice();
        self.prompt = text.into();
        let arm = Arm {
            owner: self.arm_owner,
            stamp: self.owner_stamp,
        };
        self.arm = Some(arm);
        self.arm_until = Some(arm.stamp + arm_ms);
        self.sync_notice_timer(now);
    }

    /// The arm's owner changed: the old owner's arm is lost.
    pub fn set_arm_owner(&mut self, owner: u64, stamp: u64, now: u64) -> Option<Arm> {
        self.arm_owner = owner;
        self.owner_stamp = stamp;
        let lost = match self.arm {
            Some(arm) if arm.owner != owner => self.lose_arm(now),
            _ => None,
        };
        self.follow(now);
        lost
    }

    /// The owner zeroed or replaced its stamp: the second press, any other key or a new listing ends the prompt.
    pub fn owner_stamp(&mut self, stamp: u64, now: u64) {
        self.owner_stamp = stamp;
        if self.arm.is_some_and(|arm| arm.stamp != stamp) {
            self.end_arm(now);
        }
        self.follow(now);
    }

    /// StatusBar.endArm: the prompt goes and the notice under it takes its whole time again.
    pub(super) fn end_arm(&mut self, now: u64) {
        self.arm = None;
        self.arm_until = None;
        self.prompt.clear();
        self.sync_notice_timer(now);
    }

    /// The bar ending an arm ends it at its owner too: the caller zeroes that owner's stamp if it still holds this one.
    pub(super) fn lose_arm(&mut self, now: u64) -> Option<Arm> {
        let arm = self.arm?;
        self.end_arm(now);
        if self.arm_owner == arm.owner && self.owner_stamp == arm.stamp {
            self.owner_stamp = 0;
        }
        Some(arm)
    }
}
