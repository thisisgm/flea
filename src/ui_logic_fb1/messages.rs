//! One window's message service: notices, the error queue, an arm's prompt and activities, driven by a caller's millisecond clock.
use super::status::{
    activity_changed, arm_live, cancel_activity, errors_at, notice_shown, transient_of, with_error,
    Activity, Arm, ErrorEntry, Hint, Shown, Slot,
};
mod arm;
mod line;
pub use line::Line;

/// How long a plain notice stands before it leaves on its own.
pub const MESSAGE_MS: u64 = 4000;

/// What escape did to the strip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Escape {
    Dismissed,
    Cancel(u64),
    /// The displayed transfer's cancel is already sent; the key is still consumed.
    Cancelling,
    Ignored,
}

/// What a clock tick ended: the notice, and an arm whose owner must zero its stamp.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tick {
    pub notice_cleared: bool,
    pub arm_lost: Option<Arm>,
}

#[derive(Clone, Debug, Default)]
pub struct Messages {
    path: String,
    preset: String,
    errors: Vec<ErrorEntry>,
    error_serial: u64,
    notice: String,
    notice_hint: Hint,
    notice_serial: u64,
    notice_until: Option<u64>,
    /// An explanation the user's own key asked for, drawn over the standing errors until it leaves.
    notice_first: bool,
    prompt: String,
    arm: Option<Arm>,
    arm_until: Option<u64>,
    arm_owner: u64,
    owner_stamp: u64,
    activities: Vec<Activity>,
    search_line: String,
    retry_line: String,
    /// The three edges the notice clock follows: an error drawn, a sticky line, a running search.
    edges: (bool, bool, bool),
}

impl Messages {
    pub fn new(preset: &str) -> Self {
        Self {
            preset: preset.into(),
            ..Self::default()
        }
    }

    pub fn errors(&self) -> &[ErrorEntry] {
        &self.errors
    }
    pub fn notice(&self) -> &str {
        &self.notice
    }
    pub fn notice_serial(&self) -> u64 {
        self.notice_serial
    }

    // A journal invalidation changes only its matching receipt, keeping the notice's serial and deadline.
    pub fn revoke_notice_undo(&mut self, receipt: &str, primary: &str) {
        if self.notice != receipt && self.notice != primary {
            return;
        }
        self.notice = primary.into();
        self.notice_hint = match self.notice_hint {
            Hint::Undo | Hint::TransientUndo => Hint::None,
            Hint::Note(note, _) => Hint::Note(note, false),
            hint => hint,
        };
    }

    pub fn activities(&self) -> &[Activity] {
        &self.activities
    }
    pub fn arm(&self) -> Option<Arm> {
        self.arm
    }

    pub fn arm_live(&self) -> bool {
        arm_live(self.arm, self.arm_owner, self.owner_stamp)
    }

    pub fn shown(&self) -> Shown {
        if self.notice_first && !self.notice.is_empty() && !self.arm_live() {
            return transient_of(&[], &self.notice, "", false);
        }
        transient_of(&self.errors, &self.notice, &self.prompt, self.arm_live())
    }

    /// The notice with its own advice, as v0.3.12's statusActivityState notice reads it ("Copied 1 item · z undoes").
    pub fn notice_text(&self, catalog: &crate::i18n::I18n) -> String {
        let hint = match self.notice_hint {
            Hint::Dismiss | Hint::Carry => String::new(),
            hint => hint.text(catalog, &self.preset).joined(),
        };
        if self.notice.is_empty() || hint.is_empty() {
            self.notice.clone()
        } else {
            format!("{}{}{hint}", self.notice, super::status::SEP)
        }
    }

    /// The transient sentence with its own advice, before the footer splits that advice into another lane.
    pub fn transient_text(&self, catalog: &crate::i18n::I18n) -> String {
        let shown = self.shown();
        if shown.text.is_empty() || self.arm_live() {
            return shown.text;
        }
        let hint = if shown.is_error {
            self.errors[0].hint
        } else {
            self.notice_hint
        };
        // Dismiss and carry are display-only advice, outside the transient sentence's wire contract.
        if matches!(hint, Hint::Dismiss | Hint::Carry) {
            return shown.text;
        }
        let hint = hint.text(catalog, &self.preset).joined();
        if hint.is_empty() {
            shown.text
        } else {
            format!("{}{}{hint}", shown.text, super::status::SEP)
        }
    }

    fn sticky(&self) -> &str {
        match self.activities.first() {
            Some(activity) if !activity.running => &activity.text,
            _ => "",
        }
    }

    fn slot<'a>(&'a self, shown: &'a Shown) -> Slot<'a> {
        let sticky = self.sticky();
        Slot {
            transient: &shown.text,
            transient_is_error: shown.is_error,
            arm_live: self.arm_live(),
            searching: !self.search_line.is_empty(),
            search_line: &self.search_line,
            sticky_here: !sticky.is_empty(),
            sticky,
        }
    }

    /// Completion notices cannot acknowledge a failure; each error waits for its own dismissal.
    pub fn say(&mut self, text: &str, error: bool, detail: &str, hint: Hint, now: u64) {
        if text.is_empty() {
            self.dismiss(now);
            return;
        }
        self.notice_first = false;
        if error {
            with_error(&mut self.errors, text, detail, hint, None);
            self.follow(now);
        } else {
            self.notice = text.into();
            self.notice_hint = hint;
            self.notice_serial += 1;
            self.sync_notice_timer(now);
        }
    }

    /// An operation line that already ends in the undo advice: the advice leaves the sentence for its own timed target, as Status.js hintOf splits it (EY-360).
    pub fn say_receipt(
        &mut self,
        catalog: &crate::i18n::I18n,
        text: &str,
        error: bool,
        detail: &str,
        now: u64,
    ) {
        let (text, hint) = if error {
            (text.to_owned(), Hint::None)
        } else {
            super::status::split_undo(text, catalog, &self.preset)
        };
        self.say(&text, error, detail, hint, now);
    }

    /// Why a key did nothing when nothing failed: a plain notice that stands over the errors until dismissed or expired.
    pub fn explain(&mut self, text: &str, hint: Hint, now: u64) {
        self.say(text, false, "", hint, now);
        self.notice_first = !text.is_empty();
        self.follow(now);
    }

    /// A refusal that belongs to the place it was raised in, such as a location with no Trash.
    pub fn say_placed(&mut self, text: &str, hint: Hint, now: u64) {
        let place = self.path.clone();
        with_error(&mut self.errors, text, "", hint, Some(&place));
        self.follow(now);
    }

    /// A surface replacing its own standing verdict names it, since the head may be somebody else's.
    pub fn forget(&mut self, text: &str, now: u64) {
        if !text.is_empty() {
            self.errors.retain(|entry| entry.text != text);
            self.follow(now);
        }
    }

    /// A producer can replace its own failure without acknowledging identical text from another producer.
    pub fn say_tracked_error(&mut self, text: &str, now: u64) -> Option<u64> {
        if text.is_empty() {
            return None;
        }
        self.say(text, true, "", Hint::None, now);
        let serial = self.error_serial.checked_add(1)?;
        self.error_serial = serial;
        self.errors.last_mut()?.serial = serial;
        Some(serial)
    }

    pub fn forget_error(&mut self, serial: u64, now: u64) {
        if serial != 0 {
            self.errors.retain(|entry| entry.serial != serial);
            self.follow(now);
        }
    }

    /// Dismisses the head error when one is drawn, else the notice.
    pub fn dismiss(&mut self, now: u64) {
        if self.shown().is_error {
            self.errors.remove(0);
        } else {
            self.clear_notice();
        }
        self.follow(now);
    }

    /// Clears the notice only when the serial still names it.
    pub fn withdraw(&mut self, serial: u64, now: u64) {
        if serial == self.notice_serial {
            self.clear_notice();
            self.follow(now);
        }
    }

    // The notice and its hint end together, or the muted lane outlives its sentence.
    fn clear_notice(&mut self) {
        self.notice.clear();
        self.notice_hint = Hint::None;
    }

    /// Leaving a place drops its refusals and its arm.
    pub fn set_path(&mut self, path: &str, now: u64) -> Option<Arm> {
        if self.path == path {
            return None;
        }
        self.path = path.into();
        errors_at(&mut self.errors, path);
        let lost = self.lose_arm(now);
        self.follow(now);
        lost
    }

    pub fn set_search(&mut self, line: &str, now: u64) {
        self.search_line = line.into();
        self.follow(now);
    }

    pub fn set_retry(&mut self, line: &str) {
        self.retry_line = line.into();
    }

    #[allow(clippy::too_many_arguments)]
    pub fn set_activity(
        &mut self,
        owner: u64,
        text: &str,
        transfer: u64,
        running: bool,
        starting: bool,
        now: u64,
    ) {
        activity_changed(
            &mut self.activities,
            owner,
            text,
            transfer,
            running,
            starting,
        );
        self.follow(now);
    }

    /// Asks to cancel the displayed transfer once; the id goes to its owner's backend.
    pub fn cancel_transfer(&mut self) -> Option<u64> {
        cancel_activity(&mut self.activities).then(|| self.activities[0].transfer)
    }

    pub fn activity_cancelling(&mut self, owner: u64, transfer: u64) {
        if let Some(activity) = self
            .activities
            .iter_mut()
            .find(|a| a.owner == owner && a.transfer == transfer && a.running)
        {
            activity.cancelling = true;
        }
    }

    /// Escape dismisses a standing error first, then takes every press while a cancellable transfer runs, cancelling it once.
    pub fn escape(&mut self, cancellable: bool, now: u64) -> Escape {
        if self.shown().is_error {
            self.dismiss(now);
            return Escape::Dismissed;
        }
        let running = self
            .activities
            .first()
            .is_some_and(|activity| activity.running);
        if !(running && cancellable) {
            return Escape::Ignored;
        }
        match self.cancel_transfer() {
            Some(id) => Escape::Cancel(id),
            None => Escape::Cancelling,
        }
    }

    /// Restarts the notice's whole time, or stops it while the notice is hidden or carries undo.
    fn sync_notice_timer(&mut self, now: u64) {
        let shown = self.shown();
        let slot = self.slot(&shown);
        let visible = notice_shown(&slot);
        self.edges = (slot.transient_is_error, slot.sticky_here, slot.searching);
        self.notice_until =
            (!self.notice.is_empty() && visible && !self.notice_hint.stands(&self.preset))
                .then_some(now + MESSAGE_MS);
    }

    /// Any other change touches the clock only when an edge flips; a cleared notice needs no wake.
    fn follow(&mut self, now: u64) {
        let shown = self.shown();
        let slot = self.slot(&shown);
        let edges = (slot.transient_is_error, slot.sticky_here, slot.searching);
        if edges != self.edges {
            self.sync_notice_timer(now);
        } else if self.notice.is_empty() {
            self.notice_until = None;
        }
    }

    /// The next instant a timer must wake the strip, None at rest.
    pub fn next_deadline(&self) -> Option<u64> {
        match (self.notice_until, self.arm_until) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    pub fn tick(&mut self, now: u64) -> Tick {
        let mut tick = Tick::default();
        if self.notice_until.is_some_and(|until| now >= until) {
            self.clear_notice();
            self.notice_until = None;
            tick.notice_cleared = true;
        }
        if self.arm_until.is_some_and(|until| now >= until) {
            tick.arm_lost = self.lose_arm(now);
        }
        tick
    }
}
