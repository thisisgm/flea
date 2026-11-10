//! Eject and power-off for the DEVICES service, judged on the listing that follows gio, never on its exit code.
use super::devices::{failure, Devices, Effect, Proc, EJECT_VERDICT_MS, POWER_OFF_WAIT_MS};
use super::process::{Job, Outcome};
use flea::ui_logic_device_rail::eject::{blockers, sentence, verdict, Verdict};
use flea::ui_logic_device_rail::power::{
    drives_holding, eject_command, mount_error, power_off_disk, power_off_queue, written_sectors,
    Op, DETACH_FAILED_EXIT,
};
use flea::ui_logic_device_rail::{Kind, Note, RailEntry};

impl Devices {
    /// A release holds its own volume, and an eject or power-off holds every volume on its drive.
    pub(super) fn release_note(&self, device: &str) -> Option<Note> {
        if self.unmounting && device == self.unmount_device {
            Some(Note::new("device.still_unmounting", &[&self.unmount_label]))
        } else if !self.eject_device.is_empty() && self.on_ejecting_drive(device) {
            Some(Note::new("device.still_ejecting", &[&self.eject_label]))
        } else {
            None
        }
    }

    fn on_ejecting_drive(&self, device: &str) -> bool {
        device == self.eject_device
            || drives_holding(&self.listing, device)
                .iter()
                .any(|drive| self.eject_drives.contains(drive))
    }

    /// Unmount, which is not Eject: a fixed disk stays and only its filesystem goes away.
    pub fn unmount(&mut self, index: usize) -> Vec<Effect> {
        if self.running(Proc::Mount) {
            return vec![Effect::Message(
                Note::new("device.another_mounting", &[]),
                false,
            )];
        }
        if !self.power_disk.is_empty() {
            return vec![Effect::Message(
                Note::new("device.still_ejecting", &[&self.eject_label]),
                false,
            )];
        }
        let Some(e) = self
            .entries
            .get(index)
            .filter(|e| e.kind == Kind::Volume && e.mounted)
            .cloned()
        else {
            return Vec::new();
        };
        if self.running(Proc::Unmount) {
            return vec![Effect::Message(
                Note::new("device.still_unmounting", &[&self.unmount_label]),
                false,
            )];
        }
        // One release at a time: a second Quiesce would replace the held eject child.
        if self.running(Proc::Eject) || !self.eject_device.is_empty() {
            return vec![Effect::Message(
                Note::new("device.still_ejecting", &[&self.eject_label]),
                false,
            )];
        }
        self.unmount_label = e.label.clone();
        self.unmount_device = e.device.clone();
        self.unmounting = true;
        vec![
            Effect::Quiesce(e.path.clone()),
            self.start(Proc::Unmount, Job::plain(&["gio", "mount", "-u", &e.path])),
        ]
    }

    fn arm_eject(&mut self, e: &RailEntry) {
        self.eject_device = e.device.clone();
        // Read when armed: a crypt locked mid-eject leaves the listing, and its drive stays held.
        self.eject_drives = drives_holding(&self.listing, &e.device);
        self.eject_label = e.label.clone();
        self.eject_loop = e.loop_device;
        self.detach_failed = false;
        self.verdict_from = 0;
    }

    /// Eject by mount point; a non-removable USB disk powers off instead, so nothing remounts it.
    pub fn eject(&mut self, index: usize, now: u64) -> Vec<Effect> {
        if self.running(Proc::Mount) {
            return vec![Effect::Message(
                Note::new("device.another_mounting", &[]),
                false,
            )];
        }
        let Some(e) = self
            .entries
            .get(index)
            .filter(|e| e.kind == Kind::Volume && e.mounted)
            .cloned()
        else {
            return Vec::new();
        };
        if self.running(Proc::Eject) || !self.eject_device.is_empty() {
            return vec![Effect::Message(
                Note::new("device.still_ejecting", &[&self.eject_label]),
                false,
            )];
        }
        // One release at a time: a second Quiesce would replace the held unmount child.
        if self.running(Proc::Unmount) {
            return vec![Effect::Message(
                Note::new("device.still_unmounting", &[&self.unmount_label]),
                false,
            )];
        }
        let disk = power_off_disk(&e.device);
        if !e.loop_device && !disk.is_empty() && !e.media_removable {
            return self.power_off(&e, &disk, now);
        }
        self.arm_eject(&e);
        // The notice goes ahead of the quiesce, so a quiesce failure said after it is not replaced at once.
        vec![
            Effect::Message(Note::new("device.ejecting", &[&e.label]), false),
            Effect::Quiesce(e.path.clone()),
            self.start(
                Proc::Eject,
                Job {
                    argv: eject_command(&e),
                    ..Job::default()
                },
            ),
        ]
    }

    fn power_off(&mut self, e: &RailEntry, disk: &str, now: u64) -> Vec<Effect> {
        if self.running(Proc::Unmount) {
            return vec![Effect::Message(
                Note::new("device.still_unmounting_try_again", &[&e.label]),
                false,
            )];
        }
        self.arm_eject(e);
        self.power_disk = disk.to_string();
        self.power_queue = power_off_queue(&self.listing, disk, &self.entries);
        self.power_sectors.clear();
        let mut out = vec![
            Effect::Message(Note::new("device.ejecting", &[&e.label]), false),
            Effect::Quiesce(e.path.clone()),
        ];
        out.extend(self.power_off_next(now));
        out
    }

    /// Every leg restarts the deadline, so it bounds a stalled leg and not a flushing chain.
    pub(super) fn power_off_next(&mut self, now: u64) -> Vec<Effect> {
        self.power_deadline = Some(now + POWER_OFF_WAIT_MS);
        if self.power_queue.is_empty() {
            self.power_leg.clear();
            let disk = self.power_disk.clone();
            return vec![self.start(Proc::Eject, Job::plain(&["gio", "mount", "-t", &disk]))];
        }
        let leg = self.power_queue.remove(0);
        self.power_leg = leg.path.clone();
        vec![self.start(
            Proc::Unmount,
            Job::plain(&["gio", "mount", "-u", &leg.path]),
        )]
    }

    /// A grown write counter is progress, so the deadline restarts instead of ending the chain.
    pub fn note_power_sectors(&mut self, body: &str, now: u64) {
        if self.power_disk.is_empty() {
            return;
        }
        let sectors = written_sectors(body);
        if sectors.is_empty() {
            return;
        }
        if !self.power_sectors.is_empty()
            && sectors != self.power_sectors
            && self.power_deadline.is_some()
        {
            self.power_deadline = Some(now + POWER_OFF_WAIT_MS);
        }
        self.power_sectors = sectors;
    }

    pub(super) fn end_chain(&mut self) {
        self.power_deadline = None;
        self.power_disk.clear();
        self.power_queue.clear();
        self.power_leg.clear();
        self.eject_device.clear();
        self.eject_drives.clear();
    }

    /// Whether the moved panes' volume is still mounted: a leg not yet tried, or with `refused` the leg that failed.
    pub(super) fn volume_pending(&self, refused: bool) -> bool {
        let volume = &self.moved.0;
        (refused && *volume == self.power_leg)
            || self.power_queue.iter().any(|leg| leg.path == *volume)
    }

    /// Every volume the armed release takes down: the row's own and, for an eject or power-off, the rest of its drive.
    pub fn release_volumes(&self, volume: &str) -> Vec<String> {
        let mut volumes = vec![volume.to_owned()];
        if !self.eject_device.is_empty() {
            let drive = self.entries.iter().filter(|e| {
                e.mounted
                    && !e.path.is_empty()
                    && e.path != volume
                    && self.on_ejecting_drive(&e.device)
            });
            volumes.extend(drive.map(|e| e.path.clone()));
        }
        volumes
    }

    /// The panes a release moved go back only while its volume is still mounted.
    pub(super) fn put_back(&mut self, mounted: bool) -> Vec<Effect> {
        let moved = std::mem::take(&mut self.moved.1);
        if mounted && !moved.is_empty() {
            vec![Effect::Restore(moved)]
        } else {
            Vec::new()
        }
    }

    /// A failed step of an eject stands as its verdict, so the next eject's verdict withdraws it.
    pub(super) fn chain_failure(&mut self, headline: Option<Note>, stderr: &str) -> Vec<Effect> {
        let mut out: Vec<Effect> = self
            .last_verdict
            .take()
            .map(Effect::Forget)
            .into_iter()
            .collect();
        self.last_verdict = headline.clone();
        out.push(failure(headline, stderr));
        out
    }

    /// The window refused the release before its child started, so nothing it armed stays held.
    pub fn abandon(&mut self) {
        self.running[Proc::Unmount as usize] = false;
        self.running[Proc::Eject as usize] = false;
        self.unmounting = false;
        self.verdict_deadline = None;
        self.end_chain();
        self.moved.1.clear();
    }

    pub(super) fn power_off_expired(&mut self) -> Vec<Effect> {
        if self.power_disk.is_empty()
            || (!self.running(Proc::Unmount) && !self.running(Proc::Eject))
        {
            return Vec::new();
        }
        let kill = if self.running(Proc::Unmount) {
            self.unmount_ended_late = true;
            Proc::Unmount
        } else {
            self.eject_ended_late = true;
            Proc::Eject
        };
        self.verdict_deadline = None;
        let label = self.eject_label.clone();
        // A killed leg may have unmounted its volume already, so only legs never tried count as mounted.
        let mounted = self.volume_pending(false);
        self.end_chain();
        let note = Note::new("device.eject_unfinished", &[&label]);
        let mut out = vec![Effect::Kill(kill)];
        out.extend(self.replace_verdict(note, true));
        out.extend(self.put_back(mounted));
        out
    }

    fn replace_verdict(&mut self, note: Note, error: bool) -> Vec<Effect> {
        let mut out = Vec::new();
        if let Some(last) = self.last_verdict.take() {
            out.push(Effect::Forget(last));
        }
        self.last_verdict = Some(note.clone());
        out.push(Effect::Message(note, error));
        out
    }

    /// Only a listing started after gio exited is a witness; one that cannot be judged settles nothing.
    pub(super) fn judge_eject(&mut self, body: &str) -> Vec<Effect> {
        if self.eject_device.is_empty()
            || self.verdict_from == 0
            || self.listings_started < self.verdict_from
        {
            return Vec::new();
        }
        let v = verdict(body, &self.eject_device);
        if v == Verdict::Unknown {
            return Vec::new();
        }
        let others = blockers(body, &self.eject_device);
        self.report_eject(v, &others, body)
    }

    pub(super) fn report_eject(
        &mut self,
        v: Verdict,
        others: &[String],
        body: &str,
    ) -> Vec<Effect> {
        if self.eject_device.is_empty() {
            return Vec::new();
        }
        self.power_deadline = None;
        self.verdict_deadline = None;
        let shown = if v == Verdict::Safe && self.detach_failed {
            Verdict::Attached
        } else {
            v
        };
        let (note, error) = sentence(shown, &self.eject_label, others, self.eject_loop);
        let mut out = Vec::new();
        if !error {
            let (device, label) = (self.eject_device.clone(), self.eject_label.clone());
            out.extend(self.lock_crypt(&device, &label, body).unwrap_or_default());
        }
        self.eject_device.clear();
        self.eject_drives.clear();
        out.extend(self.replace_verdict(note, error));
        out.extend(self.put_back(shown == Verdict::Mounted && others.is_empty()));
        out
    }

    pub fn eject_exited(&mut self, outcome: &Outcome, now: u64) -> Vec<Effect> {
        self.running[Proc::Eject as usize] = false;
        if std::mem::take(&mut self.eject_ended_late) {
            return self.poll(now);
        }
        if outcome.code != 0 && !self.power_disk.is_empty() {
            let headline = mount_error(Op::Eject, outcome.code, &outcome.stderr, &self.eject_label)
                .unwrap_or_else(|| Note::new("device.eject_failed", &[&self.eject_label]));
            self.end_chain();
            self.verdict_deadline = None;
            // Power-off runs once every leg is unmounted, so no volume is left to return to.
            self.moved.1.clear();
            let mut out = self.chain_failure(Some(headline), &outcome.stderr);
            out.extend(self.poll(now));
            return out;
        }
        self.power_disk.clear();
        self.detach_failed = self.eject_loop && outcome.code == DETACH_FAILED_EXIT;
        self.verdict_from = self.listings_started + 1;
        self.verdict_deadline = Some(now + EJECT_VERDICT_MS);
        self.poll(now)
    }

    /// The guard state in one line, so a failed eject names its guard through ipc.
    pub fn eject_state(&self) -> String {
        let last = self
            .last_verdict
            .as_ref()
            .map(Note::english)
            .unwrap_or_default();
        format!(
            "{}|{}|{}|{}|{}|{}|{}",
            self.eject_device,
            self.eject_label,
            self.power_disk,
            self.power_queue.len(),
            last,
            self.running(Proc::Unmount),
            self.running(Proc::Eject)
        )
    }
}
