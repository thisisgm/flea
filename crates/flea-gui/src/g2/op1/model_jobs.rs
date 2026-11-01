//! Job actions and collision state on the existing operations owner.
use super::*;

#[derive(Clone)]
pub(crate) struct Receipt {
    text: String,
    primary: String,
}

impl Receipt {
    fn new(catalog: &I18n, preset: &str, text: &str) -> Self {
        let hint = flea::ui_logic_op1::undo_hint(catalog, preset);
        Self {
            text: text.into(),
            primary: text
                .strip_suffix(&hint)
                .filter(|_| !hint.is_empty())
                .unwrap_or(text)
                .into(),
        }
    }

    pub(crate) fn owns(&self, notice: &str) -> bool {
        notice.is_empty() || notice == self.text || notice == self.primary
    }

    pub(crate) fn revoke_undo(&self, messages: &mut flea::ui_logic_fb1::messages::Messages) {
        messages.revoke_notice_undo(&self.text, &self.primary);
    }
}

impl Operations {
    pub fn job_action(&mut self, id: u64, action: jobs::Action) {
        if action == jobs::Action::Cancel
            && self.wire.transfer.running
            && self.wire.transfer.id == id
        {
            self.cancel_transfer();
            return;
        }
        if action == jobs::Action::Cancel
            && !self
                .jobs
                .rows
                .iter()
                .any(|row| row.id == id && jobs::unfinished(row))
        {
            return;
        }
        self.outbox.push(jobs::request(action.op(), id));
    }

    pub fn cancel_emptying(&mut self) {
        if self.emptying.running {
            self.outbox.push(format!(
                "{{\"c\":\"transfercancel\",\"id\":{}}}",
                self.emptying.id
            ));
        }
    }

    pub fn cancel_transfer(&mut self) {
        if self.wire.transfer.cancel_enabled() && !self.transfer_cancelling {
            self.transfer_cancelling = true;
            if let Some(job) = self
                .jobs
                .rows
                .iter_mut()
                .find(|job| job.id == self.wire.transfer.id)
            {
                job.state = "cancelling".into();
            }
            self.outbox.push(format!(
                "{{\"c\":\"transfercancel\",\"id\":{}}}",
                self.wire.transfer.id
            ));
        }
    }

    // Jobs can finish before the transfer reply reaches this owner; only that reply ends its presentation.
    pub(super) fn transfer_job(
        &mut self,
        catalog: &I18n,
        mut job: flea::ui_logic_op1::JobRow,
    ) -> flea::ui_logic_op1::JobRow {
        let transfer = &self.wire.transfer;
        if !transfer.running || transfer.id != job.id {
            return job;
        }
        if !jobs::unfinished(&job) {
            self.pending_transfer_job = Some(job.clone());
            job.head = self
                .jobs
                .rows
                .iter()
                .find(|row| row.id == job.id)
                .map(|row| row.head.clone())
                .unwrap_or_else(|| transfer.head(catalog));
            job.state = "running".into();
            job.undo = false;
        }
        if self.transfer_cancelling {
            job.state = "cancelling".into();
        }
        job
    }

    pub(super) fn transfer_jobs(
        &mut self,
        catalog: &I18n,
        jobs: Vec<flea::ui_logic_op1::JobRow>,
    ) -> bool {
        let mut jobs: Vec<_> = jobs
            .into_iter()
            .map(|job| self.transfer_job(catalog, job))
            .collect();
        let transfer = &self.wire.transfer;
        if transfer.running && !jobs.iter().any(|job| job.id == transfer.id) {
            if let Some(job) = self.jobs.rows.iter().find(|job| job.id == transfer.id) {
                jobs.push(job.clone());
            }
        }
        self.jobs.snapshot(jobs)
    }

    // An extract ends on its archive line, never a transfer reply, so that line ends the card and lets the held job row land.
    pub fn extract_done(&mut self, id: u64) -> bool {
        if !self.wire.transfer.extract || self.wire.transfer.id != id {
            return false;
        }
        self.wire.transfer = Default::default();
        self.release_transfer_job();
        self.sticky.clear();
        self.changed = true;
        true
    }

    pub(super) fn release_transfer_job(&mut self) {
        if let Some(job) = self.pending_transfer_job.take() {
            self.jobs.update(job);
        }
    }

    pub(super) fn transfer_done(
        &mut self,
        catalog: &I18n,
        done: &flea::ui_logic_op1::TransferDone,
    ) {
        if self.emptying.finish(done.id) {
            return;
        }
        if let transfer::Fold::Done { transfer, error } = self.wire.done(done.id, done.failed) {
            self.release_transfer_job();
            self.transfer_cancelling = false;
            let undo = self
                .jobs
                .rows
                .iter()
                .find(|job| job.id == done.id && jobs::state(job) == jobs::State::Done)
                .map(|job| job.undo);
            let mut line = lines::done_line(catalog, &self.preset, &transfer, done);
            if undo == Some(false) {
                let hint = flea::ui_logic_op1::undo_hint(catalog, &self.preset);
                if let Some(receipt) = line.strip_suffix(&hint).filter(|_| !hint.is_empty()) {
                    line = receipt.to_owned();
                }
            }
            self.transfer_receipt = undo.map(|_| Receipt::new(catalog, &self.preset, &line));
            self.sticky.clear();
            self.say(line.clone(), error);
            if error {
                self.say(line, false);
            }
        }
    }

    // The dd pair: the first press arms and says so, the second inside ARM_MS trashes.
    pub fn trash_key(&mut self, catalog: &I18n, rows: usize, now: u64) -> Option<String> {
        match self.arm.press(now) {
            trash::Press::Armed => Some(trash::trash_prompt(catalog, &self.preset, rows)),
            trash::Press::Act => None,
        }
    }

    // What the IPC reader reports for the collision card.
    pub fn collide_state(&self) -> String {
        let quote = |s: &str| collide::text(s);
        match &self.dialog {
            Some(Dialog::Collide {
                title,
                names,
                more,
                merge,
                focus,
            }) => {
                let names: Vec<&str> = names.iter().map(|n| n.name.as_str()).collect();
                let buttons: Vec<String> = collide::shown(*merge)
                    .iter()
                    .map(|b| quote(b.wire()))
                    .collect();
                format!(
                    "{{\"opened\":true,\"state\":{},\"title\":{},\"names\":{},\"more\":{},\"focus\":{},\"buttons\":[{}]}}",
                    quote(self.collide.state()),
                    quote(title),
                    collide::texts(&names),
                    quote(more),
                    quote(focus.wire()),
                    buttons.join(",")
                )
            }
            _ => format!(
                "{{\"opened\":false,\"state\":{}}}",
                quote(self.collide.state())
            ),
        }
    }
}
