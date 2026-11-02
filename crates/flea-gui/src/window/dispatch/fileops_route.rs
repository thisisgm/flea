//! The dd pair and where a trash key goes: Trash, the permanent review where there is none, or a refusal in a read-only folder.
use super::ARM_OWNER;
use crate::state::Shell;
use flea::{keys::sequence::Seat, ui_logic_fb1::status::Hint, ui_logic_op1::trash};
use gpui::Context;

impl Shell {
    // Trash.js arm: the first press says what a second press inside ARM_MS does, the second trashes.
    pub(super) fn fileops_arm(&mut self, side: usize, seat: Seat, cx: &mut Context<Self>) {
        let Some(host) = self.panes[side].as_ref() else {
            return;
        };
        let has_trash = match self.fileops_route(side, &host.path) {
            trash::Route::Refuse => {
                // A refused press ends a pair armed before it, as a press in another seat would.
                self.operations.fileops.disarm();
                return self.fileops_no_trash(side, cx);
            }
            route => route == trash::Route::Trash,
        };
        let now = host.pane.now();
        let held = &mut self.operations.fileops;
        // A pair belongs to one listing, cursor, view and pane; a press anywhere else arms again.
        if held.seat != Some(seat) {
            held.arm.disarm();
        }
        held.seat = Some(seat);
        if held.arm.press(now) == trash::Press::Act {
            held.seat = None;
            self.status_messages(cx, |messages, now| messages.owner_stamp(0, now));
            return self.fileops_trash(side, None, cx);
        }
        let stamp = held.arm.stamp;
        let rows = self.fileops_targets(side).map_or(0, |(_, rows)| rows.len());
        let prompt = if has_trash {
            trash::trash_prompt(&self.locale.catalog, &self.fileops_preset(), rows)
        } else {
            trash::review_prompt(&self.locale.catalog, &self.fileops_preset())
        };
        if let Some(host) = self.panes[side].as_mut() {
            host.status = prompt.clone().into();
        }
        self.status_messages(cx, |messages, now| {
            messages.set_arm_owner(ARM_OWNER, stamp, now);
            messages.say_arm(&prompt, trash::ARM_MS, now);
        });
        cx.notify();
    }

    // Focus.js: a read-only folder has no Trash either (Mounts.trashable with the listed "w"), and a gvfs one reviews a permanent delete.
    pub(super) fn fileops_route(&self, side: usize, base: &str) -> trash::Route {
        let writable = self.panes[side]
            .as_ref()
            .and_then(|host| host.backend.as_ref())
            .is_none_or(|backend| backend.dir_writable(base));
        trash::delete_route(flea::ui_logic_pl1::mounts::trashable(base, None), writable)
    }

    // Focus.js trashRefused: the key refuses in the error role before arming, naming the key that still deletes.
    pub(super) fn fileops_no_trash(&mut self, side: usize, cx: &mut Context<Self>) {
        let text = self.locale.catalog.tr("feedback.no_trash");
        if let Some(host) = self.panes[side].as_mut() {
            host.status = text.clone().into();
        }
        self.status_messages(cx, |messages, now| {
            messages.say_placed(&text, Hint::Deletes, now)
        });
        cx.notify();
    }
}
