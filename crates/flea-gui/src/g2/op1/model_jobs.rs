//! Job actions and collision state on the existing operations owner.
use super::*;

impl Operations {
    pub fn job_action(&mut self, id: u64, action: jobs::Action) {
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
        if self.wire.transfer.cancel_enabled() {
            self.outbox.push(format!(
                "{{\"c\":\"transfercancel\",\"id\":{}}}",
                self.wire.transfer.id
            ));
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
