//! Request and operation attribution, receipts and partial publication cleanup.
use super::{
    model::{Action, Card, Focus, Model, Run},
    seam::Effect,
    wire::{self, boolean, number, text},
};
use flea::{
    i18n::I18n,
    jsondoc,
    ui_logic_op4::{pages::Pages, printing},
};

impl Model {
    pub fn can_cancel(&self) -> bool {
        !self.busy
            || self
                .run
                .as_ref()
                .is_some_and(|run| run.action != Action::Print && !run.cancel_requested)
    }

    pub fn cancel(&mut self) -> bool {
        if !self.busy {
            return self.close();
        }
        let Some(run) = &mut self.run else {
            return false;
        };
        if run.action == Action::Print || run.cancel_requested {
            return false;
        }
        run.cancel_requested = true;
        if let Some(id) = run.operation {
            self.effects.push(Effect::Send(wire::cancel(id)));
        }
        true
    }

    pub fn can_submit(&self) -> bool {
        if !self.editable() || !self.error.is_empty() {
            return false;
        }
        match &self.card {
            Some(Card::Convert(c)) => {
                c.pending.is_none()
                    && c.count_pending.is_none()
                    && !c.outputs.is_empty()
                    && !c.collision
            }
            Some(Card::Combine(c)) => {
                c.pending.is_none()
                    && !c.collision
                    && self.selection.as_ref().is_some_and(|s| c.output(s).is_ok())
            }
            Some(Card::Print(p)) => {
                p.pending.is_none()
                    && p.queried
                    && if p.destinations.is_empty() {
                        !p.setup.is_empty()
                    } else {
                        p.destinations.iter().any(|d| d.name == p.printer)
                            && p.copies.parse::<usize>().ok().is_some_and(|n| {
                                (printing::MIN_COPIES..=printing::MAX_COPIES).contains(&n)
                            })
                            && Pages::parse(&p.pages).is_ok()
                    }
            }
            None => false,
        }
    }

    pub fn submit(&mut self, catalog: &I18n) {
        if !self.can_submit() {
            return;
        }
        if let Some(Card::Print(p)) = &self.card {
            if p.destinations.is_empty() {
                self.effects.push(Effect::Launch(p.setup.clone()));
                self.close();
                return;
            }
            self.submitted_printer = p.printer_name.clone();
        }
        let ticket = self.ticket();
        let Some(line) = wire::request(self, ticket) else {
            return;
        };
        let action = match self.card {
            Some(Card::Print(_)) => Action::Print,
            Some(Card::Convert(_)) => Action::Convert,
            Some(Card::Combine(_)) => Action::Combine,
            None => return,
        };
        self.run = Some(Run {
            ticket,
            operation: None,
            cancel_requested: false,
            action,
            source: self.selection.as_ref().unwrap().files[0].path.clone(),
            last_receipt: None,
        });
        self.busy = true;
        self.clear_error();
        if action != Action::Print {
            self.effects.push(Effect::Status {
                error: false,
                text: catalog.tr("dialogs.output.converting"),
            });
        }
        self.effects.push(Effect::Send(line));
    }

    pub fn line(&mut self, line: &str, catalog: &I18n) -> bool {
        let Ok(j) = jsondoc::parse(line) else {
            return false;
        };
        let Some(kind) = text(&j, "t") else {
            return false;
        };
        if kind == "printoptions" {
            return self.printer_reply(&j, catalog);
        }
        if kind == "error" {
            let origin = text(&j, "where").unwrap_or("");
            if matches!(origin, "backend" | "read") && (self.card.is_some() || self.run.is_some()) {
                self.backend_died(catalog);
                return true;
            }
            let Some(run) = &self.run else {
                return false;
            };
            let where_ = text(&j, "where").unwrap_or("");
            let matches = number(&j, "requestId") == Some(run.ticket.request);
            // Legacy prestart errors carry only where:convert; the host routes this channel exclusively while this card waits.
            if !matches
                && !(number(&j, "requestId").is_none()
                    && run.operation.is_none()
                    && where_ == "convert"
                    && run.action != Action::Print)
            {
                return false;
            }
            self.fail(catalog, text(&j, "msg").unwrap_or(""));
            return true;
        }
        let Some(run) = &mut self.run else {
            return false;
        };
        let stem = match run.action {
            Action::Print => "print",
            Action::Convert => "convert",
            Action::Combine => "combine",
        };
        if number(&j, "requestId") != Some(run.ticket.request) {
            return false;
        }
        let Some(id) = number(&j, "id").filter(|id| *id > 0) else {
            return false;
        };
        if run.action != Action::Print
            && text(&j, "source").is_some_and(|source| source != run.source)
        {
            return false;
        }
        if kind == format!("{stem}started") {
            if run.operation.is_some() {
                return false;
            }
            if run.action == Action::Convert && text(&j, "source") != Some(run.source.as_str()) {
                return false;
            }
            run.operation = Some(id);
            if run.cancel_requested {
                self.effects.push(Effect::Send(wire::cancel(id)));
            }
            if run.action == Action::Print {
                self.card = None;
                self.focus = None;
                self.effects.push(Effect::RestoreFocus);
            }
            return true;
        }
        let done = kind == format!("{stem}done");
        if run.operation != Some(id)
            && !(done && run.operation.is_none() && boolean(&j, "ok") == Some(false))
        {
            return false;
        }
        if kind == format!("{stem}progress") && run.action != Action::Print {
            let (Some(done), Some(total), Some(name)) =
                (number(&j, "done"), number(&j, "total"), text(&j, "name"))
            else {
                return false;
            };
            if total == 0 || done > total {
                return false;
            }
            self.effects.push(Effect::Progress {
                done: done as usize,
                total: total as usize,
                name: name.into(),
            });
            return true;
        }
        if kind == "printed" && run.action == Action::Print {
            let (Some(path), Some(ok)) = (text(&j, "path"), boolean(&j, "ok")) else {
                return false;
            };
            if !self
                .selection
                .as_ref()
                .is_some_and(|s| s.files.iter().any(|f| f.path == path))
            {
                return false;
            }
            let printer = self
                .selection
                .as_ref()
                .map(|_| self.print_destination())
                .unwrap_or_default();
            let name = path.rsplit('/').next().unwrap_or(path);
            let message = if ok {
                catalog.tr_args("dialogs.output.sent", &[name, &printer])
            } else {
                catalog.tr_args(
                    "dialogs.output.error.legacy",
                    &[text(&j, "message").unwrap_or("")],
                )
            };
            self.effects.push(Effect::Status {
                error: !ok,
                text: message.clone(),
            });
            if let Some(run) = &mut self.run {
                run.last_receipt = Some((message, !ok));
            }
            return true;
        }
        if !done {
            return false;
        }
        let Some(ok) = boolean(&j, "ok") else {
            return false;
        };
        let Some(reason) = text(&j, "err") else {
            return false;
        };
        let action = run.action;
        let last_receipt = run.last_receipt.clone();
        if action != Action::Print {
            if text(&j, "source") != Some(run.source.as_str()) {
                return false;
            }
            let Some(paths) = wire::strings(&j, "paths") else {
                return false;
            };
            let Some(path) = text(&j, "path") else {
                return false;
            };
            if (ok && paths.is_empty()) || path != paths.first().map_or("", String::as_str) {
                return false;
            }
            if paths
                .iter()
                .any(|p| !p.starts_with('/') || p.contains('\0'))
            {
                return false;
            }
            if !paths.is_empty() {
                self.effects.push(Effect::Refresh { paths, select: ok });
            }
        }
        let cancelled = reason == "cancelled";
        self.effects.push(Effect::ClearProgress);
        self.run = None;
        self.busy = false;
        if cancelled {
            self.close();
        } else if !ok {
            let collision = boolean(&j, "collision").unwrap_or(false);
            match &mut self.card {
                Some(Card::Convert(c)) => c.collision = collision,
                Some(Card::Combine(c)) => c.collision = collision,
                _ => (),
            }
            if action == Action::Print {
                self.remember_error(catalog, "dialogs.output.error.legacy", &[reason]);
                if !last_receipt
                    .as_ref()
                    .is_some_and(|(message, error)| *error && message == &self.error)
                {
                    self.effects.push(Effect::Status {
                        error: true,
                        text: self.error.clone(),
                    });
                }
                return true;
            }
            self.fail(catalog, text(&j, "err").unwrap_or(""));
        } else if action != Action::Print {
            let path = text(&j, "path").unwrap_or("");
            self.effects.push(Effect::Status {
                error: false,
                text: catalog.tr_args(
                    "dialogs.output.completed",
                    &[&*flea::bytename::display(
                        path.rsplit('/').next().unwrap_or(path),
                    )],
                ),
            });
            self.close();
        }
        true
    }

    fn print_destination(&self) -> String {
        // The start closes the card, so the submitted destination is retained on the active run.
        self.submitted_printer.clone()
    }

    fn fail(&mut self, catalog: &I18n, reason: &str) {
        self.busy = false;
        self.run = None;
        self.set_error(catalog, "dialogs.output.error.operation", &[reason]);
        self.focus = Some(if self.card.is_some() {
            Focus::Formats
        } else {
            Focus::Cancel
        });
        self.effects.push(Effect::ClearProgress);
    }
}
