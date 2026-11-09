//! The Trash view's replies: only the answer to the request in flight is read, and a refusal names itself.
use super::rows::{self, Reply};
use super::selection::Extend;
use super::view::{Signal, View};
use super::words::{self, Snapshot};
use crate::i18n::I18n;

impl View {
    /// One trashbrowse reply; anything not answering the request in flight is dropped.
    pub fn receive(&mut self, reply: &Reply, i18n: &I18n) {
        if (!self.opened && !self.operation_active) || reply.id != self.request_id || reply.op != self.pending_op {
            return;
        }
        self.pending_op.clear();
        if !reply.ok {
            return self.refused(reply, i18n);
        }
        let body = &reply.body;
        match reply.op.as_str() {
            "list" | "window" => self.listed(reply, i18n),
            "select" => {
                self.selection.token = rows::count(body, "selectionToken").unwrap_or(0);
                self.selection.count = rows::count(body, "selectionCount").unwrap_or(0) as usize;
                self.selection.all = self.selection.token > 0;
                let (first, count) = (self.first, self.window_rows());
                self.send("window", Self::browse().number("start", first).number("count", count));
            }
            "summary" => self.bytes = Some((rows::count(body, "bytes").unwrap_or(0), rows::flag(body, "partial"))),
            "prepare" if self.confirming => {
                self.review.token = rows::count(body, "token").unwrap_or(0);
                if !self.refresh_pending {
                    self.confirmation = Some(Snapshot {
                        count: rows::count(body, "count").map(|c| c as i64),
                        all: rows::flag(body, "all"),
                        bytes: rows::count(body, "bytes").unwrap_or(0),
                        token: self.review.token,
                    });
                }
            }
            "check" if !rows::flag(body, "valid") => self.source_changed(),
            "cancel" => self.refresh(),
            "restore" | "delete" => self.finished(reply, i18n),
            _ => {}
        }
        if self.refresh_pending && !self.busy() {
            self.source_changed();
        } else if !self.busy() {
            self.ensure_window();
        }
        self.settle_ready();
    }

    fn refused(&mut self, reply: &Reply, i18n: &I18n) {
        if reply.stale && self.opened {
            self.operation_active = false;
            self.confirming = true;
            self.source_changed();
            return self.settle_ready();
        }
        let text = reply.error.clone().unwrap_or_else(|| i18n.tr("trash.error.action"));
        self.error_text = Some(text.clone());
        self.confirming = false;
        self.confirmation = None;
        self.signals.push(Signal::Status { text, error: true });
        self.operation_active = false;
        self.settle_ready();
    }

    fn listed(&mut self, reply: &Reply, i18n: &I18n) {
        let body = &reply.body;
        let Some(rows) = reply.rows() else {
            return self.refused(&Reply { error: None, stale: false, ..reply.clone() }, i18n);
        };
        self.total = rows::count(body, "total").unwrap_or(0) as usize;
        self.signals.push(Signal::Count(self.total));
        self.set_rows(rows::count(body, "start").unwrap_or(0) as usize, rows);
        if self.selection.all {
            self.selection.count = rows::count(body, "selectionCount").unwrap_or(0) as usize;
            if rows::flag(body, "selectionStale") || rows::count(body, "selectionToken").unwrap_or(0) != self.selection.token {
                self.selection.clear();
                self.signals.push(Signal::Status { text: i18n.tr("trash.error.selection_changed"), error: true });
            }
        }
        self.cursor = self.cursor.min(self.total.saturating_sub(1));
        if reply.op == "list" {
            let errors = rows::texts(body, "recoveryErrors");
            let recovered = rows::count(body, "recoveredCount").unwrap_or(0);
            if !errors.is_empty() {
                let headline = i18n.tr("trash.recovery.failed");
                self.signals.push(Signal::Result { headline, detail: errors.join("\n"), error: true });
            } else if recovered > 0 {
                let headline = words::recovered(i18n, recovered as i64);
                self.signals.push(Signal::Result { headline, detail: String::new(), error: false });
            }
            self.error_text = None;
            self.selection.keep_present(&rows::texts(body, "present"));
            if self.total == 0 {
                self.selection.all = false;
            }
            match std::mem::take(&mut self.initial_action).as_str() {
                "emptyTrash" => self.prepare(true),
                "restoreAll" => self.restore(true),
                _ => {
                    if self.landing && self.total > 0 {
                        self.choose(0, Extend::No);
                    }
                    self.send("summary", Self::browse());
                }
            }
            self.landing = false;
        }
        if let Some((index, extend)) = self.pending_selection {
            if self.row_at(index).is_some() {
                self.choose(index as isize, extend);
            }
        }
    }

    fn finished(&mut self, reply: &Reply, i18n: &I18n) {
        let body = &reply.body;
        let done = rows::count(body, "done").unwrap_or(0) as usize;
        let failed = rows::count(body, "failed").unwrap_or(0) as usize;
        let failures = body.get("failures").and_then(crate::jsondoc::Json::as_array).unwrap_or(&[]);
        let marked: Vec<(String, String)> = failures.iter()
            .filter_map(|f| Some((rows::text(f, "uri")?, rows::text(f, "identity").unwrap_or_default()))).collect();
        self.selection.failures(&marked);
        let headline = words::done_line(i18n, reply.op == "restore", done, failed);
        let detail = failures.iter().map(|f| {
            let name = rows::text(f, "name").or_else(|| rows::text(f, "uri")).unwrap_or_default();
            words::failure_line(i18n, &name, &rows::text(f, "error").unwrap_or_default())
        }).collect::<Vec<_>>().join("\n");
        self.signals.push(Signal::Result { headline, detail, error: failed > 0 });
        self.refresh();
        self.operation_active = false;
    }
}
