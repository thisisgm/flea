//! The monitor starts off the UI thread and shares the pane lifetime; transport failure releases its pending request.
use super::*;

impl TrashPane {
    pub(crate) fn watch(&mut self, cx: &mut Context<Self>) {
        let Some(start) = self.monitor_start.filter(|_| self.monitor.is_none()) else {
            return;
        };
        self.monitor = Some(cx.spawn(async move |this, cx| {
            let started = cx.background_executor().spawn(async move { start() }).await;
            let Ok((_monitor, heard)) = started else {
                let _ = this.update(cx, |pane, cx| {
                    pane.outside
                        .status(pane.catalog.tr("trash.monitor.watch_failed"), true);
                    cx.notify();
                });
                return;
            };
            while let Ok(message) = heard.recv().await {
                if this.update(cx, |pane, cx| pane.heard(message, cx)).is_err() {
                    break;
                }
            }
        }));
    }

    /// The pipe failed before its current request could answer; retain rows and release the in-flight guard.
    pub(crate) fn transport_failed(&mut self, cx: &mut Context<Self>) {
        let view = &self.host.view;
        if !view.busy() {
            return;
        }
        let line = flea::ui_logic_pl3::rows::Line::new("trashbrowse")
            .text("t", "trashbrowse")
            .number("id", view.request_id)
            .text("op", &view.pending_op)
            .flag("ok", false)
            .text("error", &self.catalog.tr("trash.error.action"))
            .finish();
        self.line(&line, cx);
    }
}
