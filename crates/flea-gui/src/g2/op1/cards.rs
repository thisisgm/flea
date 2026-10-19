//! The permanent-delete and Copy To cards' wire: each answers to the session it was opened from, never another.
use super::model::{CopyFocus, Dialog, Operations};
use flea::i18n::I18n;
use flea::ui_logic_op1::{
    collide::{self, Choice, Fat, Request},
    confirm::{Answer, Confirm, Source},
    host::Step,
    trash::{self, Review, Scope},
    Reply,
};

impl Operations {
    // The permanent card, for the menu's Delete permanently or the Trash view's own delete, from its prepare reply.
    pub fn confirm_delete(&mut self, review: Review, scope: Scope, source: Source, now: u64) {
        self.clear_card();
        self.confirm = Some(Confirm::new(source, scope, review.clone(), now));
        self.dialog = Some(Dialog::Delete {
            review,
            scope,
            focus: trash::PRIMARY,
            busy: false,
        });
    }

    // Copy To or Move To from the menu row with snapshot id `menu`.
    pub fn copy_to(&mut self, menu: u64, moving: bool, path: &str) {
        self.clear_card();
        self.dialog = Some(Dialog::CopyTo {
            menu,
            moving,
            path: path.into(),
            focus: CopyFocus::Field,
            busy: false,
        });
    }

    // A card another card replaces is answered as dismissed, so no session is left waiting on it.
    pub(super) fn clear_card(&mut self) {
        let step = match self.dialog.take() {
            Some(Dialog::Collide { .. }) => self.collide.chosen(Choice::Cancel),
            Some(Dialog::Fat { .. }) => self.collide.decide_fat(Fat::Cancel),
            Some(Dialog::CopyTo { menu, .. }) => Step::Send(menu_close(menu)),
            Some(Dialog::Delete { .. }) | None => Step::Nothing,
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

    pub fn copy_enabled(path: &str) -> bool {
        path.starts_with('/')
    }

    pub(super) fn close_copy(&mut self) {
        if let Some(Dialog::CopyTo { menu, .. }) = self.dialog.take() {
            self.outbox.push(menu_close(menu));
        }
    }

    // A full path goes to the menu's validate, whose reply names the snapshot; a relative one is refused out loud.
    pub(super) fn confirm_copy(&mut self, catalog: &I18n) {
        let Some(Dialog::CopyTo {
            menu,
            moving,
            path,
            busy: false,
            ..
        }) = self.dialog.clone()
        else {
            return;
        };
        if path.is_empty() {
            return;
        }
        if !Self::copy_enabled(&path) {
            self.say(catalog.tr("jobs.copyto.relative"), true);
            return;
        }
        if let Some(Dialog::CopyTo { busy, .. }) = &mut self.dialog {
            *busy = true;
        }
        let action = if moving { "moveTo" } else { "copyTo" };
        self.outbox.push(format!("{{\"c\":\"menuaction\",\"op\":\"validate\",\"id\":{menu},\"action\":\"{action}\",\"dest\":{}}}", collide::text(&path)));
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

    fn validated(&mut self, catalog: &I18n, reply: &Reply) {
        let Some(Dialog::CopyTo {
            menu,
            moving,
            busy: true,
            ..
        }) = self.dialog.clone()
        else {
            return;
        };
        if reply.id != menu {
            return;
        }
        self.changed = true;
        if !reply.ok {
            if let Some(Dialog::CopyTo { busy, .. }) = &mut self.dialog {
                *busy = false;
            }
            let error = if reply.error.is_empty() {
                catalog.tr("jobs.delete.failed")
            } else {
                reply.error.clone()
            };
            self.say(error, true);
            return;
        }
        self.dialog = None;
        let request = Request::new()
            .with("c", &collide::text("transfer"))
            .with("op", &collide::text(if moving { "move" } else { "copy" }))
            .with("menuId", &menu.to_string())
            .with("dest", &collide::text(&reply.dest));
        self.ask(catalog, &request, &[], false);
        self.outbox.push(menu_close(menu));
    }
}

fn menu_close(menu: u64) -> String {
    format!("{{\"c\":\"menuaction\",\"op\":\"close\",\"id\":{menu}}}")
}
