//! Transfer activity and journal availability stay independent of a completion receipt's notice clock.
use crate::state::Shell;
use gpui::{App, Context};

impl Shell {
    pub(super) fn sync_transfer_status(&mut self, side: usize, cx: &mut Context<Self>) {
        let Some(view) = self.operation_view(side) else {
            return;
        };
        let model = &view.read(cx).model;
        let (text, id, running, cancelling) = (
            if model.wire.transfer.running {
                model.sticky.clone()
            } else {
                String::new()
            },
            model.wire.transfer.id,
            model.wire.transfer.running,
            model.transfer_cancelling,
        );
        let receipt = model
            .transfer_receipt
            .as_ref()
            .filter(|_| !running && model.jobs.undo_target().is_none())
            .cloned();
        self.status_messages(cx, |messages, now| {
            if let Some(receipt) = &receipt {
                receipt.revoke_undo(messages);
            }
            let old = messages
                .activities()
                .iter()
                .find(|activity| activity.owner == side as u64);
            if old.is_none() && !running
                || old.is_some_and(|old| {
                    old.text == text
                        && old.transfer == id
                        && old.running == running
                        && old.cancelling == cancelling
                })
            {
                return;
            }
            if running && old.is_none_or(|old| old.transfer != id) {
                messages.withdraw(messages.notice_serial(), now);
            }
            messages.set_activity(side as u64, &text, id, running, false, now);
            if cancelling {
                messages.activity_cancelling(side as u64, id);
            }
        });
    }

    pub(crate) fn transfer_undo_available(&self, notice: &str, cx: &App) -> Option<bool> {
        let host = self.panes[self.focus_side].as_ref()?;
        if host.stopped || host.backend.is_none() {
            return Some(false);
        }
        let view = self.operation_view(self.focus_side)?;
        let model = &view.read(cx).model;
        if model.wire.transfer.running {
            Some(false)
        } else if model.jobs.undo_target().is_some() {
            Some(true)
        } else {
            // Only this transfer's receipt uses its journal state; unrelated producers keep their own contract.
            model
                .transfer_receipt
                .as_ref()
                .filter(|receipt| receipt.owns(notice))
                .map(|_| false)
        }
    }
}
