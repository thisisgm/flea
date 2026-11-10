//! The device source's bounded process slots, driven by pl1's existing state machines.
use super::*;
use crate::g2::device_rail::listing::Effect as ListingEffect;
use crate::g2::device_rail::process::{Event, Job, Outcome};

/// One process slot per device child plus the phone listing; the match keeps a new Proc from sharing a slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Child {
    Device(Proc),
    Gio,
    Phone,
    Sectors,
}

pub(super) const SLOTS: usize = 8;

impl Child {
    fn index(self) -> usize {
        match self {
            Child::Device(Proc::List) => 0,
            Child::Device(Proc::Mount) => 1,
            Child::Device(Proc::Unmount) => 2,
            Child::Device(Proc::Eject) => 3,
            Child::Device(Proc::Lock) => 4,
            Child::Gio => 5,
            Child::Phone => 6,
            Child::Sectors => 7,
        }
    }
}

impl Source {
    pub(super) fn apply(&mut self, effects: Vec<Effect>, cx: &mut Context<Self>) {
        self.apply_at(effects, self.mount_side, cx);
    }

    pub(super) fn apply_at(&mut self, effects: Vec<Effect>, side: usize, cx: &mut Context<Self>) {
        let mut effects = effects.into_iter();
        while let Some(effect) = effects.next() {
            match effect {
                Effect::Start(proc, job) => self.start(Child::Device(proc), job, cx),
                // The release child waits until the window has quiesced and moved the releasing pane.
                Effect::Quiesce(path) => {
                    self.after_quiesce = effects.collect();
                    self.quiesce_side = side;
                    self.events.borrow_mut().push(Request {
                        side,
                        event: super::Event::Device(Effect::Quiesce(path)),
                    });
                    cx.notify();
                    break;
                }
                Effect::ReadSectors(path) => {
                    if self.slots[Child::Sectors.index()].handle().is_none() {
                        self.start(Child::Sectors, Job::plain(&["cat", "--", &path]), cx);
                    }
                }
                Effect::Kill(proc) => {
                    if proc == Proc::List {
                        eprintln!(
                            "{}",
                            probe_timeout(
                                "lsblk",
                                crate::g2::device_rail::devices::LIST_TIMEOUT_MS
                            )
                        );
                    }
                    self.slots[Child::Device(proc).index()].stop();
                }
                effect => {
                    self.events.borrow_mut().push(Request {
                        side,
                        event: super::Event::Device(effect),
                    });
                    cx.notify();
                }
            }
        }
        self.publish(cx);
        self.arm(cx);
    }

    pub(super) fn listing_effects(&mut self, effects: Vec<ListingEffect>, cx: &mut Context<Self>) {
        let changed = !effects.is_empty();
        for effect in effects {
            match effect {
                ListingEffect::Start(job) => self.start(Child::Gio, job, cx),
                ListingEffect::Kill => {
                    eprintln!(
                        "{}",
                        probe_timeout("gio mount -li", crate::g2::device_rail::listing::TIMEOUT_MS)
                    );
                    self.slots[Child::Gio.index()].stop();
                }
                ListingEffect::Listed => {
                    self.phones.rebuild(&self.listing.text);
                }
            }
        }
        if changed {
            cx.emit(MountsChanged);
        }
    }

    pub(super) fn start(&mut self, child: Child, job: Job, cx: &mut Context<Self>) {
        let handle = self.runner.start(job);
        let events = handle.events.clone();
        let run = self.slots[child.index()].start(handle);
        self.readers.retain(|reader| !reader.is_ready());
        self.readers.push(cx.spawn(async move |this, cx| {
            while let Ok(event) = events.recv().await {
                if let Event::Exited(outcome) = event {
                    let _ = this.update(cx, |source, cx| source.exited(child, run, outcome, cx));
                    break;
                }
            }
        }));
    }

    fn exited(&mut self, child: Child, run: u64, outcome: Outcome, cx: &mut Context<Self>) {
        if !self.slots[child.index()].ended(run) {
            return;
        }
        let probe = match child {
            Child::Device(Proc::List) if !self.devices.list_timed_out => Some("lsblk"),
            Child::Gio if !self.listing.timed_out() => Some("gio mount -li"),
            _ => None,
        };
        if let Some(line) = probe.and_then(|program| probe_failure(program, &outcome)) {
            eprintln!("{line}");
        }
        let now = self.now(cx);
        match child {
            Child::Gio => {
                let changed = !self.listing.answered
                    || (!self.listing.timed_out() && self.listing.text != outcome.stdout);
                self.listing_generation += 1;
                let effects = self.listing.exited(&outcome, now);
                let empty = effects.is_empty();
                self.listing_effects(effects, cx);
                if empty {
                    cx.emit(MountsChanged);
                }
                if changed {
                    cx.notify();
                }
            }
            Child::Phone => {
                let effects = self.phones.exited(&outcome);
                self.phone_effects(effects, self.mount_side, cx);
            }
            Child::Sectors => self.devices.note_power_sectors(&outcome.stdout, now),
            Child::Device(proc) => {
                let effects = match proc {
                    Proc::List => self.devices.list_exited(&outcome),
                    Proc::Mount => self.devices.mount_exited(&outcome, now),
                    Proc::Unmount => self.devices.unmount_exited(&outcome, now),
                    Proc::Eject => self.devices.eject_exited(&outcome, now),
                    Proc::Lock => self.devices.lock_exited(&outcome, now),
                };
                self.apply(effects, cx);
                if proc == Proc::List {
                    self.hidden_listed(cx);
                }
            }
        }
        self.publish(cx);
        self.arm(cx);
    }
}

/// The stderr a probe failure line carries, so one wedged child cannot flood the log.
const STDERR_CHARS: usize = 256;

/// The line a killed probe prints, its bound read from the deadline that killed it.
fn probe_timeout(program: &str, ms: u64) -> String {
    format!("flea-gui: device probe {program} failed: timed out after {ms} ms")
}

// Sample lsblk stdout on exit 0: {"blockdevices":[{"name":"sdb1","mountpoints":["/run/media/gm/USB"]}]}.
fn probe_failure(program: &str, outcome: &Outcome) -> Option<String> {
    let reason = if outcome.code != 0 {
        format!("exit {}", outcome.code)
    } else if program == "lsblk"
        && flea::jsondoc::parse(&outcome.stdout)
            .ok()
            .and_then(|body| {
                body.get("blockdevices")
                    .and_then(Json::as_array)
                    .map(|_| ())
            })
            .is_none()
    {
        "invalid blockdevices JSON".into()
    } else {
        return None;
    };
    let detail: String = outcome
        .stderr
        .chars()
        .take(STDERR_CHARS)
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect();
    let detail = detail.split_whitespace().collect::<Vec<_>>().join(" ");
    let suffix = if detail.is_empty() {
        String::new()
    } else {
        format!(": {detail}")
    };
    Some(format!(
        "flea-gui: device probe {program} failed ({reason}){suffix}"
    ))
}

impl Drop for Source {
    fn drop(&mut self) {
        for slot in &mut self.slots {
            slot.kill();
        }
    }
}

#[cfg(test)]
#[path = "source_probe_tests.rs"]
mod tests;
