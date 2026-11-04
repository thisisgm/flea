//! Failure replies preserve the existing listing transaction owner.
use super::*;

impl Pane {
    pub fn fail(
        &mut self,
        operation: &str,
        path: &str,
        message: &str,
        mode: u32,
        now: u64,
    ) -> bool {
        // PaneWire.qml onFailed: a failure that names a folder names the pane that never listed.
        self.named |= !path.is_empty();
        if operation == "stale" || operation == "sort" {
            // A refused sort answers no pair, so its asked count leaves the queue with it.
            if operation == "sort" {
                self.swap.sorts_out.pop_front();
            }
            return false;
        }
        if matches!(operation, "scan" | "window") && !path.is_empty() && path != self.place() {
            return false;
        }
        if operation != "backend"
            && self
                .swap
                .pending
                .as_ref()
                .is_some_and(|pending| pending.parked_until.is_some())
        {
            if operation == "window" {
                self.retire_anchor();
            }
            self.swap
                .pending
                .as_mut()
                .expect("parked listing")
                .parked_until = None;
            return self.swap.can_land(now) && self.commit(now);
        }
        // A protocol error while a pair is ready came from its anchor's answer; the complete pair still lands.
        if operation == "protocol"
            && self
                .swap
                .pending
                .as_ref()
                .is_some_and(|pending| pending.ready.is_some() && pending.retained_start.is_none())
        {
            self.retire_anchor();
            return self.swap.can_land(now) && self.commit(now);
        }
        // EY-468: one unreadable line is not the listing's answer, so a listing out still lands its pair as v0.3.12 does.
        if operation == "parse" {
            return false;
        }
        if operation == "window"
            && !self
                .swap
                .pending
                .as_ref()
                .is_some_and(|pending| pending.retained_start.is_some())
        {
            if path == self.path && !self.busy() {
                // While a sort's pair keeps its count, a window error answers that sort's window, so the view's request stays out.
                if self.swap.unheld_total.is_some() && matches!(self.swap.unheld_own, Own::Sort(_))
                {
                    self.swap.unheld_own = Own::Asked;
                } else {
                    self.requests.cancel();
                }
            } else if path == self.place() {
                self.retire_anchor();
            }
            return false;
        }
        if (!self.busy() && operation != "backend")
            || (operation == "scan" && !path.is_empty() && path != self.place())
        {
            return false;
        }
        let target = self.place().to_owned();
        self.requests.cancel();
        self.clear_rows();
        self.state = State {
            kind: if operation == "scan" && message.to_lowercase().contains("permission denied") {
                Kind::Locked
            } else {
                Kind::Error
            },
            target,
            mode,
            ..State::default()
        };
        self.swap.end(now, "dropped");
        true
    }
}
