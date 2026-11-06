//! Soft re-list after same-count Trash watch noise: debounce, defer across scroll, fire when idle.
use super::*;
use std::time::Duration;

impl TrashPane {
    pub(super) fn cancel_soft_relist(&mut self) {
        self.soft_pending = false;
        self.soft_relist = None;
    }

    /// What the Trash watch heard: the rail's count, a failure, or a settled burst.
    ///
    /// An open delete card re-reviews on `Changed` immediately. A plain listing does not:
    /// GIO's trash watch can also fire while we page the Trash window during scroll, and a
    /// full re-list there looks like Trash keeps re-reading. A different item count
    /// re-lists immediately. Same-count membership swaps schedule a soft debounced
    /// re-list that scroll defers until idle.
    pub fn heard(&mut self, heard: Heard, cx: &mut Context<Self>) {
        use flea::ui_logic_pl3::monitor::{should_relist, Relist};
        let view = &self.host.view;
        let event = match &heard {
            Heard::Changed => Some(Relist::Changed),
            Heard::Count(total) => Some(Relist::Count(*total as usize)),
            Heard::Failed(_) => None,
        };
        let relist = event
            .is_some_and(|event| should_relist(view.opened, view.confirming, view.total, event));
        let soft = matches!(heard, Heard::Changed) && view.opened && !view.confirming && !relist;
        match heard {
            Heard::Count(total) => self.outside.count(total as usize),
            Heard::Failed(key) => self.outside.monitor_failed(self.catalog.tr(key)),
            Heard::Changed => {}
        }
        if relist {
            self.soft_pending = false;
            self.soft_relist = None;
            self.host.view.source_changed();
            self.pump(cx);
        } else if soft {
            self.soft_pending = true;
            self.arm_soft_relist(cx);
        }
        cx.notify();
    }

    /// Arm or refresh the soft re-list timer for the current scroll epoch.
    fn arm_soft_relist(&mut self, cx: &mut Context<Self>) {
        use flea::ui_logic_pl3::monitor::SOFT_RELIST_MS;
        let epoch = self.scroll_epoch;
        self.soft_relist = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(SOFT_RELIST_MS))
                .await;
            let _ = this.update(cx, |pane, cx| {
                if pane.scroll_epoch != epoch || !pane.soft_pending {
                    return;
                }
                if !pane.host.view.opened || pane.host.view.confirming {
                    return;
                }
                pane.soft_pending = false;
                pane.soft_relist = None;
                pane.host.view.source_changed();
                pane.pump(cx);
                cx.notify();
            });
        }));
    }

    #[cfg(test)]
    pub(crate) fn soft_pending_for_test(&self) -> bool {
        self.soft_pending
    }

    /// User scrolled: keep a pending soft re-list and run it once scrolling is idle.
    pub(crate) fn note_scroll(&mut self, cx: &mut Context<Self>) {
        self.scroll_epoch = self.scroll_epoch.wrapping_add(1);
        if self.soft_pending {
            self.arm_soft_relist(cx);
        } else {
            self.soft_relist = None;
        }
    }
}
