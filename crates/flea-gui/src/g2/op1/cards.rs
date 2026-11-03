//! The permanent-delete and Copy To cards' wire: each answers to the session it was opened from, never another.
use super::model::{CopyFocus, Dialog, Operations};
use flea::i18n::I18n;
use flea::ui_logic_op1::{
    collide::{self, Choice, Fat, Key, Request},
    confirm::{Answer, Confirm, Source},
    host::Step,
    trash::{self, Review, Scope},
    Reply,
};

impl Operations {
    pub fn new_file(&mut self, catalog: &I18n, id: u64, folder: &str) {
        self.clear_card();
        self.dialog = Some(Dialog::NewFile {
            id,
            folder: folder.into(),
            name: catalog.tr("jobs.newfile.default"),
            focus: CopyFocus::Field,
            busy: false,
            error: String::new(),
        });
    }

    // A card another card replaces is answered as dismissed, so no session is left waiting on it.
    pub(super) fn clear_card(&mut self) {
        self.detach_rclone();
        if let Some(token) = self.external_delete.take() {
            self.external_answers.push((token, None));
        }
        let step = match self.dialog.take() {
            Some(Dialog::Collide { .. }) => self.collide.chosen(Choice::Cancel),
            Some(Dialog::Fat { .. }) => self.collide.decide_fat(Fat::Cancel),
            Some(Dialog::CopyTo { menu, .. }) => Step::Send(menu_close(menu)),
            Some(Dialog::Delete { .. } | Dialog::NewFile { .. } | Dialog::SendTo { .. }) | None => {
                Step::Nothing
            }
        };
        if let Step::Send(line) | Step::SendSpent(line) = step {
            self.outbox.push(line);
        }
        self.end_review();
    }

    // A confirmation with no delete out ends with its card, drawn or still waiting on a fresh review; a delete out is held aside in its own channel's slot.
    fn end_review(&mut self) {
        let Some(c) = self.confirm.take() else {
            return;
        };
        if c.deleting().is_some() {
            if c.is_trash() {
                self.trash_held = Some(c);
            } else {
                self.menu_held = Some(c);
            }
        } else {
            let line = c.cancel(&mut self.requests);
            self.outbox.push(line);
        }
    }

    // A menu's refresh keeps the card drawn and busy, as v0.3.12's refreshDeletion does; another card open ends the review instead.
    fn hold_delete_card(&mut self, review: Review, scope: Scope) {
        if matches!(self.dialog, None | Some(Dialog::Delete { .. })) {
            self.dialog = Some(Dialog::Delete {
                session: self.review_session,
                review,
                scope,
                focus: trash::Button::Cancel,
                busy: true,
            });
        } else {
            self.end_review();
        }
    }

    // Delete sends the reviewed token once and hides the card; Cancel, the default, tells the session it was dismissed.
    pub fn press_delete(&mut self, button: trash::Button) {
        if !matches!(self.dialog, Some(Dialog::Delete { .. })) {
            return;
        }
        if let Some(token) = self.external_delete {
            if button == trash::Button::Delete
                && !matches!(&self.dialog, Some(Dialog::Delete { review, busy: false, .. }) if trash::delete_enabled(review, self.emptying.running))
            {
                return;
            }
            self.external_delete = None;
            self.external_answers
                .push((token, (button == trash::Button::Delete).then_some(token)));
            self.dialog = None;
            return;
        }
        if button == trash::Button::Cancel {
            self.clear_card();
            return;
        }
        let running = self.emptying.running;
        if let Some(line) = self
            .confirm
            .as_mut()
            .and_then(|c| c.delete(&mut self.requests, running))
        {
            self.outbox.push(line);
            self.dialog = None;
        }
    }

    pub(super) fn walk_new_file_focus(&mut self, key: Key) {
        let Some(Dialog::NewFile {
            focus, busy, name, ..
        }) = &mut self.dialog
        else {
            return;
        };
        // A busy card has nothing to walk to: the field stays focused, as v0.3.12 leaves no control focused.
        let order: &[_] = if *busy {
            &[CopyFocus::Field]
        } else if name.is_empty() {
            &[CopyFocus::Field, CopyFocus::Cancel]
        } else {
            &[CopyFocus::Field, CopyFocus::Cancel, CopyFocus::Confirm]
        };
        *focus = collide::walk(order, *focus, key, CopyFocus::Field);
    }

    // Escape and Cancel close the card unless its create is out, as v0.3.12 keeps it open until the reply.
    pub(super) fn close_new_file(&mut self) {
        if matches!(self.dialog, Some(Dialog::NewFile { busy: false, .. })) {
            self.dialog = None;
        }
    }

    // Create sends v0.3.12's newfile request once the name is a plain filename; a bad name says so in the card.
    pub(super) fn confirm_new_file(&mut self, catalog: &I18n) {
        let Some(Dialog::NewFile {
            id,
            folder,
            name,
            busy: false,
            ..
        }) = self.dialog.clone()
        else {
            return;
        };
        if name.is_empty() {
            return;
        }
        // MenuActionDialog.qml:170 gives "." and ".." the slash sentence too.
        let slash = name == "." || name == ".." || name.contains('/');
        if let Some(Dialog::NewFile { busy, error, .. }) = &mut self.dialog {
            if slash {
                *error = catalog.tr("jobs.newfile.slash");
                return;
            }
            *busy = true;
            error.clear();
        }
        self.changed = true;
        self.outbox.push(format!(
            "{{\"c\":\"newfile\",\"op\":\"newFile\",\"id\":{id},\"path\":{},\"name\":{}}}",
            collide::text(&folder),
            collide::text(&name)
        ));
    }

    // The create's reply: ok ends the card and hands the path to the window; a refusal keeps the card with its sentence.
    fn new_file_reply(&mut self, catalog: &I18n, reply: &Reply) {
        let Some(Dialog::NewFile { id, busy: true, .. }) = self.dialog else {
            return;
        };
        if reply.id != id {
            return;
        }
        self.changed = true;
        if reply.ok && !reply.path.is_empty() {
            self.dialog = None;
            self.new_file_landed = Some(reply.path.clone());
            return;
        }
        let error = if reply.error.is_empty() {
            catalog.tr("jobs.delete.failed")
        } else {
            reply.error.clone()
        };
        if let Some(Dialog::NewFile {
            busy, error: shown, ..
        }) = &mut self.dialog
        {
            *busy = false;
            *shown = error;
        }
    }

    // A Trash or menu reply: the Copy To card's validate, or the permanent card's delete, check or fresh review.
    pub(super) fn card_reply(
        &mut self,
        catalog: &I18n,
        trash_reply: bool,
        reply: &Reply,
        now: u64,
    ) -> bool {
        self.changed = false;
        if !trash_reply && reply.op == "validate" {
            self.validated(catalog, reply);
            return false;
        }
        if !trash_reply && reply.op == "newFile" {
            self.new_file_reply(catalog, reply);
            return false;
        }
        let Some(confirm) = self.confirm.as_mut() else {
            return self.held_reply(catalog, trash_reply, reply, now);
        };
        let answer = confirm.reply(catalog, trash_reply, reply, now, &mut self.requests);
        if answer == Answer::Ignored {
            return self.held_reply(catalog, trash_reply, reply, now);
        }
        self.changed = true;
        let (review, scope, trash) = (confirm.review.clone(), confirm.scope, confirm.is_trash());
        match answer {
            Answer::Ignored | Answer::Kept => {}
            // A review that lands under another card is never drawn later out of context: it ends here.
            Answer::Shown if matches!(self.dialog, None | Some(Dialog::Delete { .. })) => {
                self.dialog = Some(Dialog::Delete {
                    session: self.review_session,
                    review,
                    scope,
                    focus: trash::PRIMARY,
                    busy: false,
                });
            }
            Answer::Shown => self.end_review(),
            Answer::Refresh(line) => {
                self.outbox.push(line);
                if trash {
                    self.drop_delete_card();
                } else {
                    self.hold_delete_card(review, scope);
                }
            }
            Answer::Dropped(line) => {
                self.confirm = None;
                self.outbox.extend(line);
                self.drop_delete_card();
            }
            Answer::Refused { error, close } => {
                self.confirm = None;
                self.outbox.extend(close);
                self.drop_delete_card();
                self.say(error, true);
            }
            Answer::Deleted { close, said } => {
                self.confirm = None;
                self.outbox.extend(close);
                if let Some(text) = said {
                    self.say_detail(text, reply.failed > 0, reply.error.clone());
                }
            }
        }
        false
    }

    // A reply to a delete held aside: its start and its refusal land here, nothing is drawn for it.
    fn held_reply(&mut self, catalog: &I18n, trash_reply: bool, reply: &Reply, now: u64) -> bool {
        // Each channel's slot reads the reply; the channel check in Confirm::reply keeps the other quiet.
        for trash in [true, false] {
            let answer = match if trash {
                &mut self.trash_held
            } else {
                &mut self.menu_held
            } {
                Some(held) => held.reply(catalog, trash_reply, reply, now, &mut self.requests),
                None => continue,
            };
            self.changed |= answer != Answer::Ignored;
            let slot = if trash {
                &mut self.trash_held
            } else {
                &mut self.menu_held
            };
            match answer {
                Answer::Deleted { close, said } => {
                    *slot = None;
                    self.outbox.extend(close);
                    if let Some(text) = said {
                        self.say_detail(text, reply.failed > 0, reply.error.clone());
                    }
                }
                Answer::Refused { error, close } => {
                    *slot = None;
                    self.outbox.extend(close);
                    self.say(error, true);
                }
                // The card that asked has moved on, so a stale review or a dropped reply is not re-asked.
                Answer::Refresh(_) | Answer::Dropped(_) => *slot = None,
                Answer::Ignored | Answer::Kept | Answer::Shown => {}
            }
        }
        false
    }

    fn drop_delete_card(&mut self) {
        if matches!(self.dialog, Some(Dialog::Delete { .. })) {
            self.dialog = None;
        }
    }
}

fn menu_close(menu: u64) -> String {
    format!("{{\"c\":\"menuaction\",\"op\":\"close\",\"id\":{menu}}}")
}

#[path = "cards_copy.rs"]
mod copy;
#[path = "cards_open.rs"]
mod open;
