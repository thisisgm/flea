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
                        .monitor_failed(pane.catalog.tr("trash.monitor.watch_failed"));
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

    /// One backend line on this session: a trashbrowse reply or a trashthumbed answer.
    pub fn line(&mut self, text: &str, cx: &mut Context<Self>) {
        let Ok(message) = jsondoc::parse(text) else {
            return;
        };
        match message.get("t").and_then(Json::as_str) {
            Some("trashbrowse") => {
                if let Some(reply) = Reply::read(message) {
                    if reply.id == self.host.view.request_id
                        && reply.op == self.host.view.pending_op
                        && reply.ok
                        && matches!(reply.op.as_str(), "list" | "window")
                    {
                        self.rows_request = reply.id;
                    }
                    let now = if self.host.sweeping() {
                        super::super::join_sweep::wall_ms()
                    } else {
                        self.now()
                    };
                    if let Some(flea::ui_logic_trash_browser::host::Swept::Finished {
                        day, ..
                    }) = self.host.receive(&reply, &self.catalog, now)
                    {
                        self.outside.swept(day);
                    }
                }
            }
            Some("trashthumbed") => {
                let id = message.get("id").and_then(Json::as_f64).unwrap_or(0.) as u64;
                let file = message.get("file").and_then(Json::as_str).unwrap_or("");
                self.host.view.thumbnailed(id, file);
            }
            _ => return,
        }
        self.pump(cx);
    }

    /// The pipe failed before its current request could answer; retain rows and release the in-flight guard.
    pub(crate) fn transport_failed(&mut self, cx: &mut Context<Self>) {
        let view = &self.host.view;
        let Some((id, op)) = self.host.sweep_request().or_else(|| {
            view.busy()
                .then_some((view.request_id, view.pending_op.as_str()))
        }) else {
            return;
        };
        let line = flea::ui_logic_trash_browser::rows::Line::new("trashbrowse")
            .text("t", "trashbrowse")
            .number("id", id)
            .text("op", op)
            .flag("ok", false)
            .text("error", &self.catalog.tr("trash.error.action"))
            .finish();
        self.line(&line, cx);
    }
}
