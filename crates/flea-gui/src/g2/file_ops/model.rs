//! The window's operations state: the one open card, the transfer, Empty Trash and the Jobs registry.
use super::press::Press;
use flea::i18n::I18n;
use flea::ui_logic_file_ops::{
    collide::{self, Choice, Fat, Key, Request},
    confirm::Confirm,
    host::{Host, Step},
    jobs::{self, Jobs},
    lines, parse, transfer,
    trash::{self, Emptying, Review, Scope},
    Collision, Event, Rename,
};

#[derive(Clone, Debug, PartialEq)]
pub enum Dialog {
    Collide {
        title: String,
        names: Vec<Collision>,
        more: String,
        merge: bool,
        focus: Choice,
    },
    Fat {
        title: String,
        renames: Vec<Rename>,
        note: String,
        focus: Fat,
    },
    // Busy while a menu's refreshDelete is out: the card stays drawn, Cancel is live and Delete is out of reach.
    Delete {
        session: u64,
        review: Review,
        scope: Scope,
        focus: trash::Button,
        busy: bool,
    },
    // The menu snapshot the row was opened from; busy while its validate is out.
    CopyTo {
        menu: u64,
        moving: bool,
        path: String,
        focus: CopyFocus,
        busy: bool,
        // v0.3.12 MenuActionDialog errorText: the last refusal, drawn under the field until the next submit.
        error: String,
    },
    SendTo {
        menu: u64,
        path: String,
        focus: CopyFocus,
        busy: bool,
        card: crate::g2::rclone_send::seams::Card,
    },
    // The folder's background New File row; id names its request, busy while create is out, error is the last refusal's sentence.
    NewFile {
        id: u64,
        folder: String,
        name: String,
        focus: CopyFocus,
        busy: bool,
        error: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyFocus {
    Field,
    Cancel,
    Confirm,
}

// A status line the window shows, and whether it is an error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Said {
    pub text: String,
    pub error: bool,
    // The backend's own sentence under the headline, as v0.3.12's operationResult detail; empty when none.
    pub detail: String,
}

pub struct Operations {
    pub(crate) rclone_target: Option<crate::g2::rclone_send::join::Target>,
    pub preset: String,
    pub collide: Host,
    pub dialog: Option<Dialog>,
    pub wire: transfer::Wire,
    pub(super) pending_transfer_job: Option<flea::ui_logic_file_ops::JobRow>,
    pub transfer_cancelling: bool,
    pub(super) transfer_receipt: Option<jobs_state::Receipt>,
    pub emptying: Emptying,
    pub jobs: Jobs,
    pub arm: trash::Arm,
    // The permanent-delete card's session, kept while its delete or a fresh review is out.
    pub confirm: Option<Confirm>,
    pub review_session: u64,
    /// A reviewed card owned by another producer; answers return to that producer without a second request counter.
    pub external_delete: Option<u64>,
    pub external_answers: Vec<(u64, Option<u64>)>,
    // A delete already out, moved aside when another card replaced its confirmation; one slot per channel, so a menu delete never drops a held Trash delete and vice versa.
    pub trash_held: Option<Confirm>,
    pub menu_held: Option<Confirm>,
    // Seam: lines for the backend; the window's transport drains them.
    pub outbox: Vec<String>,
    pub said: Vec<Said>,
    pub sticky: String,
    pub spent_cut: bool,
    pub held_listing: u64,
    pub requests: u64,
    // Seam: true while the Trash view has its own trashbrowse request out; a Trash card's check waits, as v0.3.12's !busy.
    pub trash_busy: bool,
    // False after a line that changed nothing drawn, so the host skips the redraw.
    pub changed: bool,
    // Each drawn job block with the row it was built from, rebuilt only when that row changes.
    pub blocks: Vec<(flea::ui_logic_file_ops::JobRow, jobs::Block)>,
    pub blocks_built: usize,
    // The path of a New File that just landed; the window re-reads its folder onto it.
    pub new_file_landed: Option<String>,
}

impl Operations {
    pub fn new(preset: &str) -> Self {
        Self {
            preset: preset.into(),
            rclone_target: None,
            collide: Host::default(),
            dialog: None,
            wire: transfer::Wire::default(),
            pending_transfer_job: None,
            transfer_cancelling: false,
            transfer_receipt: None,
            emptying: Emptying::default(),
            jobs: Jobs::default(),
            arm: trash::Arm::default(),
            confirm: None,
            review_session: 0,
            external_delete: None,
            external_answers: Vec::new(),
            trash_held: None,
            menu_held: None,
            outbox: Vec::new(),
            said: Vec::new(),
            sticky: String::new(),
            spent_cut: false,
            held_listing: 0,
            requests: 0,
            new_file_landed: None,
            trash_busy: false,
            changed: false,
            blocks: Vec::new(),
            blocks_built: 0,
        }
    }

    // The Jobs card's blocks: an unchanged row keeps the block already built from it.
    pub fn refresh_blocks(&mut self, catalog: &I18n) {
        super::progress::blocks(
            &mut self.blocks,
            &mut self.blocks_built,
            &self.jobs,
            catalog,
        );
    }

    pub fn open(&self) -> bool {
        self.dialog.is_some()
    }

    pub(super) fn say(&mut self, text: String, error: bool) {
        self.say_detail(text, error, String::new());
    }

    pub(super) fn say_detail(&mut self, text: String, error: bool, detail: String) {
        self.said.push(Said {
            text,
            error,
            detail,
        });
    }

    pub(super) fn step(&mut self, catalog: &I18n, step: Step) {
        match step {
            Step::Nothing => {}
            Step::Refuse => self.say(collide::waiting_refusal(catalog), false),
            Step::Send(line) => self.outbox.push(line),
            Step::SendSpent(line) => {
                self.outbox.push(line);
                self.spent_cut = true;
            }
            Step::Open {
                total,
                names,
                dest,
                merge,
            } => {
                self.clear_card();
                let title = collide::title(catalog, &names, total, &dest);
                let more = collide::more(catalog, total, names.len());
                self.dialog = Some(Dialog::Collide {
                    title,
                    names,
                    more,
                    merge,
                    focus: collide::START,
                });
            }
            Step::OpenFat {
                renames,
                fstype,
                volume,
            } => {
                self.clear_card();
                let title = collide::fat_title(catalog, renames.len() as u64, &volume, &fstype);
                let note = collide::fat_note(catalog, &fstype);
                self.dialog = Some(Dialog::Fat {
                    title,
                    renames,
                    note,
                    focus: collide::FAT_START,
                });
            }
        }
    }

    // A paste, a drop or Copy To asks here first; nothing goes out until the card is answered.
    pub fn ask(&mut self, catalog: &I18n, request: &Request, probe: &[&str], cut: bool) -> bool {
        let step = self.collide.ask(request, probe, cut, self.held_listing);
        let asked = step != Step::Refuse;
        self.step(catalog, step);
        asked
    }

    // One backend line; anything that is not an operations line is left for the other readers.
    pub fn line(&mut self, catalog: &I18n, text: &str, now: u64) -> bool {
        let Some(event) = parse(text) else {
            return false;
        };
        self.changed = true;
        match event {
            Event::Collisions {
                id,
                total,
                names,
                mergeable,
            } => {
                let step = self.collide.answered(id, total, names, mergeable);
                self.step(catalog, step);
            }
            Event::MergeCollisions { id, total, names } => {
                let step = self.collide.answered_merge(id, total, names);
                self.step(catalog, step);
            }
            Event::FatNames {
                id,
                renames,
                fstype,
                volume,
            } => {
                let step = self.collide.answered_fat(id, renames, fstype, volume);
                self.step(catalog, step);
            }
            Event::TransferStarted {
                id,
                n,
                moving,
                extract,
            } => {
                self.release_transfer_job();
                self.transfer_cancelling = false;
                self.transfer_receipt = None;
                self.wire.started(id, moving, n, extract);
                self.sticky = lines::progress_line(catalog, &self.wire.transfer);
            }
            Event::TransferProgress {
                id,
                index,
                name,
                bytes,
                total,
                scanned,
                phase,
                drive,
                items,
                checked,
                item_size,
            } => {
                if items
                    && self
                        .emptying
                        .sampled(id, &phase, &name, item_size, index, total, checked, now)
                {
                    return true;
                }
                if self
                    .wire
                    .progress(id, index, &name, bytes, total, scanned, &phase, &drive)
                    == transfer::Fold::Drawn
                {
                    self.sticky = lines::progress_line(catalog, &self.wire.transfer);
                }
            }
            Event::TransferItem {
                id,
                index,
                name,
                ok,
                err,
            } => match self.wire.item(id, index, &name, ok, &err, now) {
                transfer::Fold::Failed { name, error } => {
                    let moving = self.wire.transfer.moving;
                    self.say(
                        lines::transfer_failure(catalog, moving, &name, &error),
                        true,
                    );
                    self.sticky = lines::progress_line(catalog, &self.wire.transfer);
                }
                transfer::Fold::Drawn => {
                    self.sticky = lines::progress_line(catalog, &self.wire.transfer)
                }
                _ => {}
            },
            Event::TransferDone(done) => self.transfer_done(catalog, &done),
            Event::Trashed { ok, failed, err } => {
                if ok > 0 || failed > 0 {
                    self.sticky.clear();
                    self.say(
                        lines::trashed(catalog, &self.preset, ok, failed, &err),
                        ok == 0,
                    );
                }
            }
            Event::TrashStarted { .. } => {}
            // Only the Trash delete this card sent starts Empty Trash's progress; no card is touched.
            Event::TrashDeleteStarted { id, request_id } => {
                match self
                    .trash_delete(request_id)
                    .map(|c| c.review.count.unwrap_or(0))
                {
                    Some(count) => self.emptying = Emptying::started(id, count, now),
                    None => self.changed = false,
                }
            }
            // Shared with the Trash view and the menu, so other readers still get these lines.
            Event::TrashBrowse(reply) => return self.card_reply(catalog, true, &reply, now),
            Event::MenuAction(reply) => return self.card_reply(catalog, false, &reply, now),
            Event::Jobs { jobs, .. } => self.changed = self.transfer_jobs(catalog, jobs),
            Event::Job { job, .. } => {
                let job = self.transfer_job(catalog, job);
                self.changed = self.jobs.update(job);
            }
            Event::JobError { error, code, .. } => {
                let text = jobs::refusal(catalog, &error, &code);
                self.jobs.refused(&text);
                self.say(text, true);
            }
            Event::Undone { op, ok } if ok => {
                let line = lines::undone(catalog, &op);
                self.say(line, false);
            }
            Event::Redone { op, ok } if ok => {
                let line = lines::with_undo(catalog, &self.preset, lines::redone(catalog, &op));
                self.say(line, false);
            }
            Event::Undone { .. } | Event::Redone { .. } => {}
        }
        true
    }

    // The beat that lands held item lines, and the open confirmation's identity check.
    pub fn tick(&mut self, catalog: &I18n, now: u64) -> bool {
        let landed = self.wire.tick(now);
        if landed {
            self.sticky = lines::progress_line(catalog, &self.wire.transfer);
        }
        let busy = self.trash_busy;
        if let Some(line) = self
            .confirm
            .as_mut()
            .filter(|c| !(busy && c.is_trash()))
            .and_then(|c| c.tick(now, &mut self.requests))
        {
            self.outbox.push(line);
        }
        landed
    }

    // Seam: part of the Trash view's busy; the view's open and close still send, as v0.3.12 does, and a later request supersedes this one quietly.
    pub fn trash_request_out(&self) -> bool {
        [
            self.confirm.as_ref(),
            self.trash_held.as_ref(),
            self.menu_held.as_ref(),
        ]
        .into_iter()
        .flatten()
        .any(Confirm::trash_out)
    }

    // The Trash delete whose request id this is, held on the card or moved aside under another card.
    fn trash_delete(&self, request_id: u64) -> Option<&Confirm> {
        [
            self.confirm.as_ref(),
            self.trash_held.as_ref(),
            self.menu_held.as_ref(),
        ]
        .into_iter()
        .flatten()
        .find(|c| c.is_trash() && c.deleting() == Some(request_id))
    }

    // Seam: the Trash view closed; a Trash confirmation with no delete out ends with it, never drawn later over another view.
    pub fn trash_closed(&mut self) {
        if self
            .confirm
            .as_ref()
            .is_some_and(|c| c.is_trash() && c.deleting().is_none())
        {
            self.confirm = None;
            if matches!(self.dialog, Some(Dialog::Delete { .. })) {
                self.dialog = None;
            }
        }
    }

    // When the host's timer next has work: the held items' beat or the confirmation's check.
    pub fn due(&self) -> Option<u64> {
        let beat = self.wire.held_since.map(|since| since + transfer::BEAT_MS);
        let check = self
            .confirm
            .as_ref()
            .filter(|c| !(self.trash_busy && c.is_trash()))
            .and_then(Confirm::due);
        beat.into_iter().chain(check).min()
    }

    pub fn failed(&mut self, place: &str, input: &str) {
        if self.collide.failed(place, input)
            && matches!(
                self.dialog,
                Some(Dialog::Collide { .. } | Dialog::Fat { .. })
            )
        {
            self.dialog = None;
        }
    }

    // A key while a card is open: the card takes every key, so nothing reaches the listing under it.
    pub fn key(&mut self, catalog: &I18n, name: &str, shift: bool) -> bool {
        let Some(dialog) = self.dialog.clone() else {
            return false;
        };
        let key = collide::key(name, shift);
        match dialog {
            Dialog::Collide { merge, focus, .. } => match key {
                Key::Escape => self.choose(catalog, Choice::Cancel),
                Key::Activate => self.choose(catalog, focus),
                _ => self.focus_collide(collide::moved(focus, key, merge)),
            },
            Dialog::Fat { focus, .. } => match key {
                Key::Escape => self.choose_fat(catalog, Fat::Cancel),
                Key::Activate => self.choose_fat(catalog, focus),
                _ => {
                    if let Some(Dialog::Fat { focus: f, .. }) = &mut self.dialog {
                        *f = collide::walk(&collide::FAT_BUTTONS, focus, key, collide::FAT_START);
                    }
                }
            },
            Dialog::Delete { focus, busy, .. } => match key {
                Key::Escape => self.press_delete(trash::Button::Cancel),
                Key::Activate => self.press_delete(focus),
                // A busy card has nothing to walk to: Cancel stays focused.
                _ if busy => {}
                _ => {
                    if let Some(Dialog::Delete { focus: f, .. }) = &mut self.dialog {
                        *f = collide::walk(&trash::BUTTONS, focus, key, trash::PRIMARY);
                    }
                }
            },
            Dialog::CopyTo { focus, .. } | Dialog::SendTo { focus, .. } => match key {
                Key::Escape => self.close_copy(),
                // The field is a text input: Space types, and only Enter submits from it.
                Key::Activate if focus == CopyFocus::Field && name == "space" => {}
                Key::Activate if focus == CopyFocus::Cancel => self.close_copy(),
                Key::Activate => self.confirm_copy(catalog),
                Key::Tab | Key::Backtab => self.walk_copy_focus(key),
                // Typing and selection belong to the mounted platform input handler.
                _ => {}
            },
            Dialog::NewFile { focus, .. } => match key {
                Key::Escape => self.close_new_file(),
                Key::Activate if focus == CopyFocus::Field && name == "space" => {}
                Key::Activate if focus == CopyFocus::Cancel => self.close_new_file(),
                Key::Activate => self.confirm_new_file(catalog),
                Key::Tab | Key::Backtab => self.walk_new_file_focus(key),
                _ => {}
            },
        }
        true
    }

    // A pointer press on a button does what Enter does on it; a disabled button's press does nothing.
    pub fn press(&mut self, catalog: &I18n, press: Press) {
        match press {
            Press::Collide(choice) if matches!(self.dialog, Some(Dialog::Collide { .. })) => {
                self.choose(catalog, choice)
            }
            Press::Fat(choice) if matches!(self.dialog, Some(Dialog::Fat { .. })) => {
                self.choose_fat(catalog, choice)
            }
            Press::Delete(button) => self.press_delete(button),
            Press::CopyCancel
                if matches!(
                    self.dialog,
                    Some(Dialog::CopyTo { .. } | Dialog::SendTo { .. })
                ) =>
            {
                self.close_copy()
            }
            Press::CopyConfirm
                if matches!(
                    self.dialog,
                    Some(Dialog::CopyTo { .. } | Dialog::SendTo { .. })
                ) =>
            {
                self.confirm_copy(catalog)
            }
            Press::NewFileCancel if matches!(self.dialog, Some(Dialog::NewFile { .. })) => {
                self.close_new_file()
            }
            Press::NewFileConfirm if matches!(self.dialog, Some(Dialog::NewFile { .. })) => {
                self.confirm_new_file(catalog)
            }
            Press::Job(id, action) => self.job_action(id, action),
            _ => {}
        }
    }

    fn focus_collide(&mut self, next: Choice) {
        if let Some(Dialog::Collide { focus, .. }) = &mut self.dialog {
            *focus = next;
        }
    }

    pub fn choose(&mut self, catalog: &I18n, choice: Choice) {
        self.dialog = None;
        let step = self.collide.chosen(choice);
        self.step(catalog, step);
    }

    pub fn choose_fat(&mut self, catalog: &I18n, choice: Fat) {
        self.dialog = None;
        let step = self.collide.decide_fat(choice);
        self.step(catalog, step);
    }

    // Each block's own button names only its job.
}

#[path = "model_jobs.rs"]
mod jobs_state;
