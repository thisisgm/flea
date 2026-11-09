//! The DEVICES group's service: lsblk polled into rail rows, and the gio mount and unmount legs.
use super::process::{Job, Outcome};
use flea::ui_logic_device_rail::devices::{
    host_label, mount_timer_start, parse_devices, LSBLK_ARGV,
};
use flea::ui_logic_device_rail::mounts::same_entries;
use flea::ui_logic_device_rail::power::{mount_detail, mount_error, Op, Queued};
use flea::ui_logic_device_rail::{Kind, Note, RailEntry};

/// A child the service runs; one of each at a time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Proc {
    List,
    Mount,
    Unmount,
    Eject,
    Lock,
}

/// What the service asks of its driver and the window.
#[derive(Debug)]
pub enum Effect {
    Start(Proc, Job),
    Kill(Proc),
    Opened(String),
    Message(Note, bool),
    Operation {
        headline: Option<Note>,
        detail: String,
    },
    Quiesce(String),
    UnlockRequested {
        device: String,
        label: String,
        size: u64,
        fstype: String,
    },
    Forget(Note),
    ReadSectors(String),
    /// A release ended with its volume still mounted, so the panes it moved go back.
    Restore(Vec<Moved>),
    /// A phone's Unmount waits until the window has checked its transfers against the phone.
    ReleasePhone {
        key: String,
        uri: String,
        label: String,
    },
}

/// A pane a release moved off its volume: its side, the folder it left and the place it landed in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Moved {
    pub side: usize,
    pub from: String,
    pub to: String,
}

pub const POLL_MS: u64 = 5_000;
pub const LIST_TIMEOUT_MS: u64 = 10_000;
pub const MOUNT_TIMEOUT_MS: u64 = 15_000;
pub const EJECT_VERDICT_MS: u64 = 15_000;
pub const POWER_OFF_WAIT_MS: u64 = 15_000;

#[derive(Debug, Default)]
pub struct Devices {
    pub entries: Vec<RailEntry>,
    pub revision: u64,
    pub show_unmounted: bool,
    pub polling: bool,
    pub hostname: String,
    pub first_answered: bool,
    pub list_timed_out: bool,
    pub listing: String,
    pub(super) running: [bool; 5],
    pub(super) listings_started: u64,
    pub(super) verdict_from: u64,
    stream_pending: bool,
    poll_armed: bool,
    next_poll: u64,
    list_deadline: Option<u64>,
    mount_deadline: Option<u64>,
    pub(super) verdict_deadline: Option<u64>,
    pub(super) power_deadline: Option<u64>,
    pending_open_device: String,
    pending_open_label: String,
    pub(super) unmount_label: String,
    pub(super) unmount_device: String,
    pub(super) unmounting: bool,
    pub(super) eject_device: String,
    pub(super) eject_drives: Vec<String>,
    pub(super) eject_label: String,
    pub(super) eject_loop: bool,
    pub(super) detach_failed: bool,
    pub(super) power_disk: String,
    pub(super) power_queue: Vec<Queued>,
    pub(super) power_sectors: String,
    pub(super) power_leg: String,
    /// The volume the release in flight moved panes off, and those panes.
    pub(super) moved: (String, Vec<Moved>),
    pub(super) unmount_ended_late: bool,
    pub(super) eject_ended_late: bool,
    pub(super) last_verdict: Option<Note>,
    pub(super) lock_label: String,
    pub(super) lock_backing: String,
    pub(super) lock_queue: Vec<(String, String)>,
    pub(super) lock_checks: Vec<(String, String, u64)>,
}

impl Devices {
    pub fn new(hostname: &str) -> Self {
        Self {
            show_unmounted: true,
            polling: true,
            hostname: hostname.to_string(),
            ..Self::default()
        }
    }

    pub fn running(&self, proc: Proc) -> bool {
        self.running[proc as usize]
    }

    pub(super) fn start(&mut self, proc: Proc, job: Job) -> Effect {
        self.running[proc as usize] = true;
        Effect::Start(proc, job)
    }

    /// A release in flight: an eject awaiting its verdict, a power-off chain, or a user unmount.
    pub fn releasing(&self) -> bool {
        !self.eject_device.is_empty() || self.unmounting
    }

    fn poll_wanted(&self) -> bool {
        self.polling
            || !self.first_answered
            || self.releasing()
            || !self.pending_open_device.is_empty()
            || !self.power_disk.is_empty()
            || !self.lock_checks.is_empty()
    }

    pub fn poll(&mut self, now: u64) -> Vec<Effect> {
        if self.running(Proc::List) || self.stream_pending {
            return Vec::new();
        }
        self.list_timed_out = false;
        self.stream_pending = true;
        self.listings_started += 1;
        self.list_deadline = Some(now + LIST_TIMEOUT_MS);
        let mut out = vec![self.start(Proc::List, Job::plain(LSBLK_ARGV))];
        if !self.power_disk.is_empty() {
            let base = flea::ui_logic_device_rail::power::sys_base(&self.power_disk);
            out.push(Effect::ReadSectors(format!("/sys/block/{base}/stat")));
        }
        out
    }

    /// The earliest moment tick has work, so a driver arms one timer instead of polling frames.
    pub fn deadline(&self) -> Option<u64> {
        let poll = (self.poll_wanted() && self.poll_armed).then_some(self.next_poll);
        [
            poll,
            self.list_deadline,
            self.mount_deadline,
            self.verdict_deadline,
            self.power_deadline,
        ]
        .into_iter()
        .flatten()
        .min()
    }

    /// Every timer the service keeps, as deadlines against one clock.
    pub fn tick(&mut self, now: u64) -> Vec<Effect> {
        let mut out = Vec::new();
        let wanted = self.poll_wanted();
        if wanted && (!self.poll_armed || now >= self.next_poll) {
            self.next_poll = now + POLL_MS;
            out.extend(self.poll(now));
        }
        self.poll_armed = wanted;
        if self.list_deadline.is_some_and(|at| now >= at) {
            self.list_deadline = None;
            if self.running(Proc::List) {
                self.list_timed_out = true;
                out.push(Effect::Kill(Proc::List));
            }
        }
        if self.mount_deadline.is_some_and(|at| now >= at) {
            self.mount_deadline = None;
            if !self.pending_open_device.is_empty() {
                self.pending_open_device.clear();
                out.push(Effect::Message(
                    Note::new("device.no_folder", &[&self.pending_open_label]),
                    true,
                ));
            }
        }
        if self.verdict_deadline.is_some_and(|at| now >= at) {
            self.verdict_deadline = None;
            out.extend(self.report_eject(
                flea::ui_logic_device_rail::eject::Verdict::Unknown,
                &[],
                "",
            ));
        }
        if self.power_deadline.is_some_and(|at| now >= at) {
            self.power_deadline = None;
            out.extend(self.power_off_expired());
        }
        out
    }

    pub fn list_exited(&mut self, outcome: &Outcome) -> Vec<Effect> {
        self.running[Proc::List as usize] = false;
        self.list_deadline = None;
        self.stream_pending = false;
        let witness =
            (!self.list_timed_out && outcome.code == 0).then_some(outcome.stdout.as_str());
        let mut out = self.judge_locks(witness);
        if !self.list_timed_out {
            out.extend(self.judge_eject(&outcome.stdout));
            self.listing = outcome.stdout.clone();
            out.extend(self.rebuild());
        }
        self.first_answered = true;
        out
    }

    pub fn rebuild(&mut self) -> Vec<Effect> {
        let rows: Vec<RailEntry> = parse_devices(&self.listing, self.show_unmounted)
            .into_iter()
            .map(|mut r| {
                if r.kind == Kind::Disk {
                    r.label = host_label(&r.label, &self.hostname);
                }
                r.uuid.clear();
                r
            })
            .collect();
        if !same_entries(&self.entries, &rows) {
            self.entries = rows;
            self.revision += 1;
        }
        self.open_when_mounted()
    }

    fn open_when_mounted(&mut self) -> Vec<Effect> {
        if self.pending_open_device.is_empty() {
            return Vec::new();
        }
        let Some(e) = self
            .entries
            .iter()
            .find(|e| e.device == self.pending_open_device && e.mounted)
        else {
            return Vec::new();
        };
        let path = e.path.clone();
        self.pending_open_device.clear();
        self.mount_deadline = None;
        vec![Effect::Opened(path)]
    }

    /// A disk or mounted volume opens; a locked one asks for its passphrase; any other mounts first.
    pub fn activate(&mut self, index: usize) -> Vec<Effect> {
        let Some(e) = self.entries.get(index).cloned() else {
            return Vec::new();
        };
        if let Some(note) = self.release_note(&e.device) {
            return vec![Effect::Message(note, false)];
        }
        if e.locked {
            let request = Effect::UnlockRequested {
                device: e.device,
                label: e.label,
                size: e.size.unwrap_or(0),
                fstype: e.fstype,
            };
            return vec![request];
        }
        if e.mounted {
            return vec![Effect::Opened(e.path)];
        }
        if self.running(Proc::Mount) {
            return vec![Effect::Message(
                Note::new("device.another_mounting", &[]),
                false,
            )];
        }
        self.pending_open_device = e.device.clone();
        self.pending_open_label = e.label.clone();
        vec![self.start(Proc::Mount, Job::plain(&["gio", "mount", "-d", &e.device]))]
    }

    pub fn mount_exited(&mut self, outcome: &Outcome, now: u64) -> Vec<Effect> {
        self.running[Proc::Mount as usize] = false;
        let label = self.pending_open_label.clone();
        let sentence = mount_error(Op::Mount, outcome.code, &outcome.stderr, &label);
        if mount_timer_start(outcome.code) || (sentence.is_none() && !label.is_empty()) {
            self.mount_deadline = Some(now + MOUNT_TIMEOUT_MS);
            return self.poll(now);
        }
        self.mount_deadline = None;
        self.pending_open_device.clear();
        let headline = sentence.unwrap_or_else(|| Note::new("device.mount_failed", &[&label]));
        vec![failure(Some(headline), &outcome.stderr)]
    }

    pub fn unmount_exited(&mut self, outcome: &Outcome, now: u64) -> Vec<Effect> {
        self.running[Proc::Unmount as usize] = false;
        self.unmounting = false;
        if std::mem::take(&mut self.unmount_ended_late) {
            return self.poll(now);
        }
        let mut out = Vec::new();
        if !self.power_disk.is_empty() {
            if outcome.code != 0 {
                let mounted = self.volume_pending(true);
                self.end_chain();
                let headline = mount_error(
                    Op::Unmount,
                    outcome.code,
                    &outcome.stderr,
                    &self.eject_label,
                );
                out.extend(self.chain_failure(headline, &outcome.stderr));
                out.extend(self.put_back(mounted));
            } else {
                out.extend(self.power_off_next(now));
            }
            out.extend(self.poll(now));
            return out;
        }
        if outcome.code != 0 {
            out.push(failure(
                mount_error(
                    Op::Unmount,
                    outcome.code,
                    &outcome.stderr,
                    &self.unmount_label,
                ),
                &outcome.stderr,
            ));
            out.extend(self.put_back(true));
        } else {
            self.moved.1.clear();
            let (device, label, listing) = (
                self.unmount_device.clone(),
                self.unmount_label.clone(),
                self.listing.clone(),
            );
            if let Some(lock) = self.lock_crypt(&device, &label, &listing) {
                out.extend(lock);
                return out;
            }
        }
        out.extend(self.poll(now));
        out
    }
}

/// A failed operation: its sentence and gio's own reason as the detail line.
pub(super) fn failure(headline: Option<Note>, stderr: &str) -> Effect {
    Effect::Operation {
        headline,
        detail: mount_detail(stderr),
    }
}
