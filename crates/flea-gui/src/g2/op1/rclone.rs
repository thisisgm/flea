//! Rclone reuses Copy To's field, focus, buttons and background Jobs presentation.
use super::model::{CopyFocus, Dialog, Operations};
use crate::g2::sd2::{join::Target, seams::Wire};
use flea::i18n::I18n;
use std::cell::RefCell;

struct Out(RefCell<Vec<String>>);
impl Wire for Out {
    fn send(&self, line: String) -> bool {
        self.0.borrow_mut().push(line);
        true
    }
}

impl Operations {
    pub(crate) fn open_rclone(&mut self, menu: u64, target: Target, catalog: &I18n) {
        let Some(card) = target.borrow().card(catalog) else {
            return;
        };
        self.clear_card();
        self.dialog = Some(Dialog::SendTo {
            menu,
            path: card.value.clone(),
            focus: CopyFocus::Field,
            busy: false,
            card,
        });
        self.rclone_target = Some(target);
    }

    pub(super) fn close_rclone(&mut self) -> bool {
        let Some(target) = self.rclone_target.take() else {
            return false;
        };
        target.borrow_mut().close_card();
        self.dialog = None;
        true
    }

    // Another card replacing Send To closes it without a cancel: a send already asked for runs on in Jobs.
    pub(super) fn detach_rclone(&mut self) {
        if let Some(target) = self.rclone_target.take() {
            target.borrow_mut().drop_card();
            self.dialog = None;
        }
    }

    pub(super) fn submit_rclone(&mut self) -> bool {
        let Some(target) = &self.rclone_target else {
            return false;
        };
        if let Some(Dialog::SendTo {
            path, busy: false, ..
        }) = &self.dialog
        {
            let out = Out(RefCell::new(Vec::new()));
            target.borrow_mut().edit(path);
            if target.borrow_mut().submit(&out) {
                self.outbox.extend(out.0.into_inner());
                if let Some(Dialog::SendTo { busy, .. }) = &mut self.dialog {
                    *busy = true;
                }
            }
        }
        true
    }

    pub(crate) fn sync_rclone(&mut self, catalog: &I18n) {
        let Some(target) = &self.rclone_target else {
            return;
        };
        let card = target.borrow().card(catalog);
        if let Some(card) = card {
            if let Some(Dialog::SendTo {
                busy,
                focus,
                card: shown,
                ..
            }) = &mut self.dialog
            {
                let waiting = !card.can_submit && !card.value.is_empty() && card.error.is_none();
                // Only a refusal hands the field back; any other update keeps the focus where the user put it.
                if *busy && !waiting {
                    *focus = CopyFocus::Field;
                }
                *busy = waiting;
                *shown = card;
            }
        } else {
            self.dialog = None;
            self.rclone_target = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // An open Send To card on remote gdrive with folder Uploads typed, as the menu leaves it.
    fn opened() -> (Target, Out, Operations, I18n) {
        let target = Target::default();
        let wire = Out(RefCell::default());
        target.borrow_mut().menu_opened(&wire);
        assert!(target
            .borrow_mut()
            .on_line(r#"{"t":"rcloneremotes","id":1,"visible":true,"remotes":["gdrive"]}"#));
        assert!(target
            .borrow_mut()
            .choose("gdrive", vec!["/owned/a.txt".into()]));
        let catalog = crate::test_catalogs::english();
        let mut operations = Operations::new("default");
        operations.open_rclone(7, target.clone(), &catalog);
        if let Some(Dialog::SendTo { path, .. }) = &mut operations.dialog {
            *path = "Uploads".into();
        }
        (target, wire, operations, catalog)
    }

    fn focus(operations: &Operations) -> Option<CopyFocus> {
        match operations.dialog {
            Some(Dialog::SendTo { focus, .. }) => Some(focus),
            _ => None,
        }
    }

    #[test]
    fn vf6_cancel_before_started_cancels_the_eventual_job_once() {
        let (target, wire, mut operations, catalog) = opened();
        operations.key(&catalog, "enter", false);
        assert_eq!(operations.outbox.len(), 1);
        operations.key(&catalog, "escape", false);
        assert!(operations.dialog.is_none());
        assert!(target
            .borrow_mut()
            .on_line(r#"{"t":"rclonestarted","requestId":2,"id":19,"ok":true,"error":""}"#));
        assert!(target.borrow_mut().cancel_closed_card(2, &wire));
        assert!(!target.borrow_mut().cancel_closed_card(2, &wire));
        assert_eq!(
            wire.0.borrow().last().unwrap(),
            r#"{"c":"jobs","op":"cancel","id":19}"#
        );
    }

    #[test]
    fn vf6_a_replacing_card_keeps_a_send_already_asked_for() {
        let (target, wire, mut operations, catalog) = opened();
        operations.key(&catalog, "enter", false);
        assert_eq!(operations.outbox.len(), 1);
        operations.copy_to(8, false, "/elsewhere");
        assert!(matches!(operations.dialog, Some(Dialog::CopyTo { .. })));
        assert!(operations.rclone_target.is_none());
        assert!(target
            .borrow_mut()
            .on_line(r#"{"t":"rclonestarted","requestId":2,"id":19,"ok":true,"error":""}"#));
        assert!(!target.borrow_mut().cancel_closed_card(2, &wire));
        assert!(wire.0.borrow().iter().all(|line| !line.contains("cancel")));
    }

    #[test]
    fn vf6_a_send_update_keeps_the_focus_and_a_refusal_returns_it_to_the_field() {
        let (target, _, mut operations, catalog) = opened();
        if let Some(Dialog::SendTo { focus, .. }) = &mut operations.dialog {
            *focus = CopyFocus::Cancel;
        }
        operations.sync_rclone(&catalog);
        assert_eq!(focus(&operations), Some(CopyFocus::Cancel));
        if let Some(Dialog::SendTo { focus, .. }) = &mut operations.dialog {
            *focus = CopyFocus::Field;
        }
        operations.key(&catalog, "enter", false);
        if let Some(Dialog::SendTo { focus, .. }) = &mut operations.dialog {
            *focus = CopyFocus::Cancel;
        }
        operations.sync_rclone(&catalog);
        assert!(matches!(
            operations.dialog,
            Some(Dialog::SendTo {
                busy: true,
                focus: CopyFocus::Cancel,
                ..
            })
        ));
        assert!(target
            .borrow_mut()
            .on_line(r#"{"t":"rclonestarted","requestId":2,"ok":false,"error":"remote refused"}"#));
        operations.sync_rclone(&catalog);
        assert!(matches!(
            operations.dialog,
            Some(Dialog::SendTo {
                busy: false,
                focus: CopyFocus::Field,
                ..
            })
        ));
    }
}
