//! Committed operation surfaces retain their whole snapshot until the shared swap permits replacement.
use super::{
    host::Overlay,
    model::{Dialog, Operations, Said},
};
use crate::pane::swap::Progress;
use flea::{
    i18n::I18n,
    ui_logic_file_ops::{
        jobs::{self, Jobs},
        JobRow,
    },
};
use gpui::{App, Context, KeyDownEvent};

#[path = "card_rate.rs"]
mod card_rate;

impl Overlay {
    pub(super) fn held_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        if !self.progress.dialog.holding(self.now(cx)) {
            return false;
        }
        let keys = &event.keystroke;
        let modified = keys.modifiers.control || keys.modifiers.alt || keys.modifiers.platform;
        if keys.key == "escape"
            || (!modified && matches!(keys.key.as_str(), "enter" | "return" | "space" | "kp_enter"))
        {
            self.cancel_held(cx);
        }
        cx.stop_propagation();
        true
    }

    pub(super) fn cancel_held(&mut self, cx: &mut Context<Self>) {
        if let (
            Some(Dialog::Delete { session: drawn, .. }),
            Some(Dialog::Delete { session: live, .. }),
        ) = (self.drawn_dialog(), &self.model.dialog)
        {
            if drawn == live {
                self.model
                    .press_delete(flea::ui_logic_file_ops::trash::Button::Cancel);
            }
        }
        self.schedule(cx);
        cx.notify();
    }

    pub(super) fn drawn_review(&self) -> Option<u64> {
        match self.drawn_dialog() {
            Some(Dialog::Delete { session, .. }) => Some(*session),
            _ => None,
        }
    }

    pub(super) fn review_pointer_matches(&self) -> bool {
        matches!(self.model.dialog, Some(Dialog::Delete { session, .. })
            if self.pressed_review == Some(session) && self.drawn_review() == Some(session))
    }

    pub(super) fn cancel_drawn_activity(&mut self) -> bool {
        if let Some(id) = self.progress.jobs.oldest().map(|job| job.id) {
            self.model.job_action(id, jobs::Action::Cancel);
            return true;
        }
        if let Some(id) = *self.progress.transfer.shown() {
            if self.model.wire.transfer.id == id {
                self.model.cancel_transfer();
            }
            return true;
        }
        false
    }

    pub fn modal(&self, cx: &App) -> bool {
        self.model.open() || self.progress.dialog.holding(self.now(cx))
    }

    /// The Jobs card, the one overlay card that stands beside the panes rather than over them.
    pub fn jobs_showing(&self, cx: &App) -> bool {
        *self.progress.card.value(self.now(cx))
            || (self.model.jobs.visible() && !self.model.jobs.folded)
    }

    pub fn showing(&self, cx: &App) -> bool {
        self.modal(cx) || self.jobs_showing(cx)
    }

    pub fn drawn_dialog(&self) -> Option<&Dialog> {
        self.progress.dialog.shown().as_ref()
    }

    pub(crate) fn drawn_jobs(&self) -> &Jobs {
        &self.progress.jobs
    }

    // A frame that brings nothing new stops before any publication; a running spiral's own frames land here.
    pub(super) fn publication_if_pending(
        &mut self,
        cx: &App,
        last: &str,
    ) -> Option<(Vec<Said>, String)> {
        let now = self.now(cx);
        self.progress.observe(&self.model, now);
        if !self.progress.pending(&self.model, now, last) {
            return None;
        }
        Some(self.progress.publication(&mut self.model, now))
    }
}

#[derive(Default)]
pub(super) struct Presentation {
    rows: Vec<(u64, Progress<Option<JobRow>>)>,
    rates: card_rate::Rates,
    pub jobs: Jobs,
    card: Progress<bool>,
    activity: Progress<bool>,
    pub dialog: Progress<Option<Dialog>>,
    transfer: Progress<Option<u64>>,
    said: Vec<Said>,
    sticky: String,
}

impl Presentation {
    pub fn observe(&mut self, model: &Operations, now: u64) {
        self.activity
            .observe(&model.jobs.visible(), model.jobs.unfinished() > 0, now);
        self.card.observe(
            &(!model.jobs.folded && model.jobs.visible()),
            !model.jobs.folded && model.jobs.unfinished() > 0,
            now,
        );
        for (at, row) in model.jobs.rows.iter().enumerate() {
            match self.rows.iter().position(|(id, _)| *id == row.id) {
                Some(found) => self.rows.swap(at, found),
                None => self.rows.insert(at, (row.id, Progress::default())),
            }
        }
        for (id, row) in &mut self.rows {
            let latest = model.jobs.rows.iter().find(|latest| latest.id == *id);
            let active = latest.is_some_and(jobs::unfinished);
            if row.target().as_ref() == latest {
                row.progress(active, now);
            } else {
                row.observe(&latest.cloned(), active, now);
            }
        }
        self.dialog.observe(
            &model.dialog,
            matches!(model.dialog, Some(Dialog::Delete { busy: true, .. })),
            now,
        );
        let transfer = model
            .wire
            .transfer
            .running
            .then_some(model.wire.transfer.id);
        if transfer.is_some() && self.transfer.target() != &transfer {
            self.transfer.restart(now);
        }
        self.transfer
            .observe(&transfer, model.wire.transfer.running, now);
    }

    // Folding or unfolding the Jobs card is the user's own act, so the card supersedes the progress hold both ways.
    pub fn toggled(&mut self, model: &Operations, now: u64) {
        self.observe(model, now);
        self.card.supersede();
    }

    pub fn due(&self, now: u64) -> Option<u64> {
        self.rows
            .iter()
            .filter_map(|(_, row)| row.due(now))
            .chain(self.card.due(now))
            .chain(self.activity.due(now))
            .chain(self.dialog.due(now))
            .chain(self.transfer.due(now))
            .chain(self.rates.due(now))
            .min()
    }

    fn holding(&self, now: u64) -> bool {
        self.card.holding(now)
            || self.activity.holding(now)
            || self.dialog.holding(now)
            || self.transfer.holding(now)
            || self.rows.iter().any(|(_, row)| row.holding(now))
    }

    pub fn commit(&mut self, now: u64) {
        let held_card = self.card.holding(now) || self.activity.holding(now);
        let was_card = !self.jobs.folded && self.jobs.visible();
        let showing = *self.card.commit(now);
        let activity = *self.activity.commit(now);
        if !held_card {
            for (_, row) in &mut self.rows {
                if activity {
                    if showing && !was_card {
                        row.reveal(now);
                    }
                    row.commit(now);
                } else {
                    row.hide();
                }
            }
            let mut rows: Vec<_> = self
                .rows
                .iter()
                .filter_map(|(_, row)| row.shown().clone())
                .collect();
            if showing {
                self.rates.publish(&mut rows, now);
            } else {
                self.rates.clear();
            }
            if self.jobs.rows != rows {
                self.jobs.snapshot(rows);
            }
        }
        self.jobs.folded = !showing;
        self.dialog.commit(now);
        self.transfer.commit(now);
        self.rows
            .retain(|(_, row)| row.target().is_some() || row.shown().is_some());
    }

    // What a publication would carry now: a command, a spent cut, a publishable notice, or a sticky change.
    fn pending(&self, model: &Operations, now: u64, last: &str) -> bool {
        if !model.outbox.is_empty() || model.spent_cut {
            return true;
        }
        if self.holding(now) {
            return false;
        }
        !model.said.is_empty() || !self.said.is_empty() || self.sticky_after(model, now) != last
    }

    fn sticky_after<'a>(&'a self, model: &'a Operations, now: u64) -> &'a str {
        if self.holding(now) {
            return &self.sticky;
        }
        let progress = !model.wire.transfer.running
            || *self.transfer.value(now) == Some(model.wire.transfer.id);
        if progress {
            &model.sticky
        } else {
            &self.sticky
        }
    }

    pub fn publication(&mut self, model: &mut Operations, now: u64) -> (Vec<Said>, String) {
        if self.holding(now) {
            self.said.append(&mut model.said);
            return (Vec::new(), self.sticky.clone());
        }
        self.said.append(&mut model.said);
        let progress = !model.wire.transfer.running
            || *self.transfer.value(now) == Some(model.wire.transfer.id);
        if progress {
            self.sticky.clone_from(&model.sticky);
        }
        (std::mem::take(&mut self.said), self.sticky.clone())
    }
}

pub(super) fn blocks(
    built: &mut Vec<(JobRow, jobs::Block)>,
    count: &mut usize,
    jobs: &Jobs,
    catalog: &I18n,
) {
    let mut rows = jobs.blocks();
    if built.iter().all(|(row, _)| rows.next() == Some(row)) && rows.next().is_none() {
        return;
    }
    let mut kept = std::mem::take(built);
    for row in jobs.blocks() {
        match kept.iter().position(|(old, _)| old == row) {
            Some(at) => built.push(kept.swap_remove(at)),
            None => {
                *count += 1;
                built.push((row.clone(), jobs::block(catalog, row)));
            }
        }
    }
}
