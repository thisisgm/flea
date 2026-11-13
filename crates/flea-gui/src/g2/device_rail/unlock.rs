//! The unlock card's state machine: the passphrase goes to udisksctl on stdin, and every exit leaves the drive as found.
use super::field::Field;
use super::process::{Job, Outcome};
use super::secret::Secret;
use flea::ui_logic_device_rail::unlock::{
    crypto_name, lock_argv, lock_note, mount_note, parse_mounted, parse_unlocked, refusal_note,
    unlock_argv, unlock_failed_note, unlocked_note, unlocking_note, wrong_note,
};
use flea::ui_logic_device_rail::Note;

/// A child an unlock card runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Proc {
    Unlock,
    Mount,
    Lock,
}

/// What the card asks its driver to do.
#[derive(Debug)]
pub enum Effect {
    Start(Proc, Job),
    Kill(Proc),
    WithholdInput(Proc),
    Unlocked {
        path: String,
        label: String,
        note: Note,
    },
    Closed,
    LockFailed(Note),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Typing,
    Unlocking,
    Wrong,
}

/// The child in flight: relock is a failed mount's lock, abandon a Cancel whose late exit still settles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Stage {
    #[default]
    Idle,
    Unlock,
    Mount,
    Relock,
    Abandon,
}

/// A keyboard stop on the card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Field,
    Cancel,
    Unlock,
}

/// How long a submit may wait for a child that never started or an earlier attempt to settle.
pub const UNLOCK_TIMEOUT_MS: u64 = 15_000;

#[derive(Debug, Default)]
pub struct Card {
    pub opened: bool,
    pub device: String,
    pub label: String,
    pub size: u64,
    pub crypto: String,
    pub phase: Phase,
    pub note: Option<(Note, bool)>,
    pub field: Field,
    pub focus: Option<Part>,
    pub timeout_ms: u64,
    secret: Secret,
    stage: Stage,
    lock_label: String,
    quiet_lock: bool,
    abandon_device: String,
    abandon_label: String,
    queued: bool,
    started: bool,
    running: [bool; 3],
    deadline: Option<u64>,
}

impl Card {
    pub fn new() -> Self {
        Self {
            timeout_ms: UNLOCK_TIMEOUT_MS,
            crypto: "LUKS".into(),
            ..Self::default()
        }
    }

    pub fn stage(&self) -> Stage {
        self.stage
    }

    /// When the card's one bound next expires, if armed.
    pub fn deadline(&self) -> Option<u64> {
        self.deadline
    }

    pub fn queued(&self) -> bool {
        self.queued
    }

    pub fn running(&self, proc: Proc) -> bool {
        self.running[proc as usize]
    }

    pub fn open_for(&mut self, device: &str, label: &str, size: u64, fstype: &str, fsver: &str) {
        self.device = device.to_string();
        self.describe(label, size, fstype, fsver);
        self.phase = Phase::Typing;
        self.note = None;
        self.secret.clear();
        self.field.clear();
        self.field.error = false;
        self.opened = true;
        self.focus = Some(Part::Field);
    }

    /// Names the drive, so a later rail row can name a card flea --unlock opened on a bare device.
    pub fn describe(&mut self, label: &str, size: u64, fstype: &str, fsver: &str) {
        self.label = if label.is_empty() {
            self.device.clone()
        } else {
            label.to_string()
        };
        self.size = size;
        self.crypto = crypto_name(fstype, fsver);
    }

    /// Unlock is unavailable until the field shows dots, typed or held from a refused try, and while one is tried.
    pub fn unlock_available(&self) -> bool {
        self.opened && self.phase != Phase::Unlocking && self.field.masked()
    }

    /// The field, Cancel, then Unlock when it is available; while unlocking Cancel is the one stop.
    pub fn tab_stops(&self) -> Vec<Part> {
        match (self.phase, self.unlock_available()) {
            (Phase::Unlocking, _) => vec![Part::Cancel],
            (_, true) => vec![Part::Field, Part::Cancel, Part::Unlock],
            (_, false) => vec![Part::Field, Part::Cancel],
        }
    }

    pub fn step_tab(&mut self, back: bool) {
        let stops = self.tab_stops();
        let at = self.focus.and_then(|f| stops.iter().position(|s| *s == f));
        let next = match at {
            None if back => stops.len() - 1,
            None => 0,
            Some(i) if back => (i + stops.len() - 1) % stops.len(),
            Some(i) => (i + 1) % stops.len(),
        };
        self.focus = Some(stops[next]);
    }

    /// The card is untouched until the user types, submits or reads a sentence on it.
    pub fn touched(&self) -> bool {
        self.phase != Phase::Typing || self.note.is_some() || self.field.masked()
    }

    pub fn type_text(&mut self, text: &str) {
        if !self.opened || self.phase == Phase::Unlocking {
            return;
        }
        self.field.type_text(text);
        self.edited();
    }

    pub fn backspace(&mut self) {
        if !self.opened || self.phase == Phase::Unlocking {
            return;
        }
        self.field.backspace();
        self.edited();
    }

    pub(super) fn edited(&mut self) {
        if self.field.error || self.note.as_ref().is_some_and(|(_, error)| *error) {
            self.field.error = false;
            self.note = None;
        }
    }

    pub fn close(&mut self) -> Vec<Effect> {
        let mut out = Vec::new();
        self.deadline = None;
        self.queued = false;
        match self.stage {
            Stage::Unlock => {
                self.abandon();
                if !self.started {
                    out.push(Effect::WithholdInput(Proc::Unlock));
                }
            }
            Stage::Relock => self.stage = Stage::Idle,
            Stage::Mount => {
                self.abandon();
                if self.running(Proc::Mount) {
                    out.push(Effect::Kill(Proc::Mount));
                } else {
                    let (device, label) = (self.abandon_device.clone(), self.abandon_label.clone());
                    out.push(self.lock_container(false, &device, &label));
                }
            }
            Stage::Idle | Stage::Abandon => {}
        }
        self.field.clear();
        self.secret.clear();
        self.opened = false;
        out.push(Effect::Closed);
        out
    }

    fn abandon(&mut self) {
        self.stage = Stage::Abandon;
        self.abandon_device = self.device.clone();
        self.abandon_label = self.label.clone();
    }

    fn refuse(&mut self, note: Note) {
        self.deadline = None;
        self.stage = Stage::Idle;
        self.secret.clear();
        self.show_refusal(note);
    }

    fn show_refusal(&mut self, note: Note) {
        self.phase = Phase::Wrong;
        self.field.error = true;
        self.note = Some((note, true));
        self.focus = Some(Part::Field);
    }

    fn lock_container(&mut self, relock: bool, device: &str, label: &str) -> Effect {
        self.stage = if relock { Stage::Relock } else { Stage::Idle };
        self.lock_label = if label.is_empty() {
            self.label.clone()
        } else {
            label.to_string()
        };
        let device = if device.is_empty() {
            self.device.clone()
        } else {
            device.to_string()
        };
        self.running[Proc::Lock as usize] = true;
        Effect::Start(Proc::Lock, Job::c_locale(lock_argv(&device)))
    }

    pub fn submit(&mut self, now: u64) -> Vec<Effect> {
        if !self.unlock_available() {
            return Vec::new();
        }
        if self.field.is_empty() {
            self.show_refusal(wrong_note());
            return Vec::new();
        }
        self.secret = self.field.take();
        self.phase = Phase::Unlocking;
        self.focus = Some(Part::Cancel);
        self.field.error = false;
        self.note = Some((unlocking_note(&self.label), false));
        if self.running(Proc::Unlock) || self.running(Proc::Lock) || self.stage == Stage::Abandon {
            self.queued = true;
            self.deadline = Some(now + self.timeout_ms);
            return Vec::new();
        }
        vec![self.start_unlock(now)]
    }

    fn start_queued(&mut self, now: u64) -> Vec<Effect> {
        if !self.queued {
            return Vec::new();
        }
        self.queued = false;
        self.deadline = None;
        vec![self.start_unlock(now)]
    }

    fn start_unlock(&mut self, now: u64) -> Effect {
        self.started = false;
        self.stage = Stage::Unlock;
        self.quiet_lock = false;
        self.running[Proc::Unlock as usize] = true;
        self.deadline = Some(now + self.timeout_ms);
        let mut job = Job::c_locale(unlock_argv(&self.device));
        job.stdin = Some(self.secret.take().keyed_for(&self.crypto));
        Effect::Start(Proc::Unlock, job)
    }

    /// The bound ends a queued submit or a child that never started; a slow key derivation has none.
    pub fn tick(&mut self, now: u64) -> Vec<Effect> {
        if self.deadline.is_none_or(|at| now < at) {
            return Vec::new();
        }
        self.deadline = None;
        if self.queued {
            self.queued = false;
            self.secret.clear();
            self.show_refusal(unlock_failed_note(&self.label));
            return Vec::new();
        }
        if self.started || self.stage != Stage::Unlock {
            return Vec::new();
        }
        self.refuse(unlock_failed_note(&self.label));
        vec![Effect::Kill(Proc::Unlock)]
    }

    pub fn started(&mut self, proc: Proc) -> Vec<Effect> {
        if proc != Proc::Unlock {
            return Vec::new();
        }
        self.started = true;
        if self.stage == Stage::Unlock {
            Vec::new()
        } else {
            vec![Effect::WithholdInput(Proc::Unlock)]
        }
    }

    pub fn exited(&mut self, proc: Proc, outcome: &Outcome, now: u64) -> Vec<Effect> {
        self.running[proc as usize] = false;
        match proc {
            Proc::Unlock => self.unlock_exited(outcome, now),
            Proc::Mount => self.mount_exited(outcome),
            Proc::Lock => {
                let mut out = self.lock_exited(outcome.code);
                out.extend(self.start_queued(now));
                out
            }
        }
    }

    fn unlock_exited(&mut self, outcome: &Outcome, now: u64) -> Vec<Effect> {
        self.deadline = None;
        if self.stage == Stage::Abandon {
            if outcome.code == 0 && !parse_unlocked(&outcome.stdout).is_empty() {
                self.quiet_lock = true;
                let (device, label) = (self.abandon_device.clone(), self.abandon_label.clone());
                return vec![self.lock_container(true, &device, &label)];
            }
            self.stage = Stage::Idle;
            return self.start_queued(now);
        }
        if self.stage != Stage::Unlock {
            return Vec::new();
        }
        if outcome.code != 0 {
            self.refuse(refusal_note(&outcome.stderr, &self.label));
            return Vec::new();
        }
        let mapped = parse_unlocked(&outcome.stdout);
        if mapped.is_empty() {
            self.refuse(wrong_note());
            return Vec::new();
        }
        self.stage = Stage::Mount;
        self.running[Proc::Mount as usize] = true;
        let argv = ["udisksctl", "mount", "--block-device", &mapped]
            .iter()
            .map(|s| s.to_string())
            .collect();
        vec![Effect::Start(Proc::Mount, Job::c_locale(argv))]
    }

    fn mount_exited(&mut self, outcome: &Outcome) -> Vec<Effect> {
        if self.stage == Stage::Abandon {
            let (device, label) = (self.abandon_device.clone(), self.abandon_label.clone());
            return vec![self.lock_container(false, &device, &label)];
        }
        if self.stage != Stage::Mount {
            return Vec::new();
        }
        let folder = if outcome.code == 0 {
            parse_mounted(&outcome.stdout)
        } else {
            String::new()
        };
        if folder.is_empty() {
            return vec![self.lock_container(true, "", "")];
        }
        let label = self.label.clone();
        let note = unlocked_note(&label);
        self.stage = Stage::Idle;
        self.field.clear();
        self.opened = false;
        vec![
            Effect::Closed,
            Effect::Unlocked {
                path: folder,
                label,
                note,
            },
        ]
    }

    fn lock_exited(&mut self, code: i32) -> Vec<Effect> {
        let quiet = std::mem::take(&mut self.quiet_lock);
        if self.stage == Stage::Relock {
            if self.opened && !quiet {
                let label = self.lock_label.clone();
                self.refuse(mount_note(&label));
            } else {
                self.stage = Stage::Idle;
            }
        }
        if code != 0 {
            vec![Effect::LockFailed(lock_note(&self.lock_label))]
        } else {
            Vec::new()
        }
    }
}
