//! One card lifetime; request identities keep old replies out of new drafts.
use super::{
    combine::Combine,
    convert::Convert,
    printing::Print,
    seam::{Effect, Ticket},
};
use flea::{
    i18n::I18n,
    ui_logic_print_convert::{
        output::{self, Kind},
        selection::Selection,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Print,
    Convert,
    Combine,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Formats,
    Check,
    Printer,
    Copies,
    Pages,
    Duplex,
    Grayscale,
    Preview,
    Layout,
    Columns,
    Spacing,
    Background,
    Format,
    Name,
    Cancel,
    Submit,
}

pub enum Card {
    Print(Print),
    Convert(Convert),
    Combine(Combine),
}

pub struct Run {
    pub ticket: Ticket,
    pub operation: Option<u64>,
    pub cancel_requested: bool,
    pub action: Action,
    pub source: String,
    pub last_receipt: Option<(String, bool)>,
}

enum ErrorText {
    Args(&'static str, Vec<String>),
    Count(&'static str, i64),
    Io(Option<&'static str>, String),
}

#[derive(Default)]
pub struct Model {
    pub selection: Option<Selection>,
    pub card: Option<Card>,
    pub focus: Option<Focus>,
    pub busy: bool,
    pub unavailable: bool,
    pub error: String,
    error_spec: Option<ErrorText>,
    pub run: Option<Run>,
    pub effects: Vec<Effect>,
    pub submitted_printer: String,
    serial: u64,
}

impl Model {
    pub fn ticket(&mut self) -> Ticket {
        self.serial = self
            .serial
            .checked_add(1)
            .expect("output request identity exhausted");
        Ticket {
            generation: self.selection.as_ref().map_or(0, |s| s.generation),
            request: self.serial,
        }
    }

    pub fn open(&mut self, action: Action, selection: Selection, catalog: &I18n) -> bool {
        if self.busy || self.run.is_some() || self.card.is_some() {
            return false;
        }
        let kinds = selection.kinds();
        let validation = selection.validate().and_then(|()| match action {
            Action::Print if !output::can_print(&kinds) => Err("dialogs.output.error.print_kind"),
            Action::Convert if !output::can_convert(&kinds) => {
                Err("dialogs.output.error.convert_kind")
            }
            Action::Combine if !output::can_combine(&kinds) => {
                Err("dialogs.output.error.image_count")
            }
            Action::Convert | Action::Combine if !selection.one_folder() => {
                Err("dialogs.output.error.folder")
            }
            _ => Ok(()),
        });
        if let Err(key) = validation {
            self.set_error(catalog, key, &[]);
            return false;
        }
        self.unavailable = false;
        self.clear_error();
        self.focus = Some(match action {
            Action::Print => Focus::Submit,
            Action::Convert if kinds[0] == Kind::Office => Focus::Submit,
            Action::Convert => Focus::Formats,
            Action::Combine => Focus::Submit,
        });
        self.card = Some(match action {
            Action::Print => Card::Print(Print::new()),
            Action::Convert => Card::Convert(Convert::new(&selection)),
            Action::Combine => Card::Combine(Combine::new(&selection)),
        });
        self.selection = Some(selection);
        match action {
            Action::Print => self.query_printer(None),
            Action::Convert => self.probe(catalog),
            Action::Combine => self.probe(catalog),
        }
        true
    }

    pub fn close(&mut self) -> bool {
        if self.busy {
            return false;
        }
        self.card = None;
        self.focus = None;
        self.effects.push(Effect::RestoreFocus);
        true
    }

    pub fn editable(&self) -> bool {
        self.card.is_some() && !self.busy && !self.unavailable
    }

    pub fn backend_died(&mut self, catalog: &I18n) {
        let pending = self.busy;
        let printing = matches!(self.run.as_ref().map(|r| r.action), Some(Action::Print))
            || matches!(self.card, Some(Card::Print(_)));
        self.busy = false;
        self.run = None;
        self.unavailable = true;
        self.focus = Some(Focus::Cancel);
        self.set_error(
            catalog,
            if printing {
                "dialogs.output.error.print_stopped"
            } else if pending {
                "dialogs.output.error.stopped_pending"
            } else {
                "dialogs.output.error.stopped"
            },
            &[],
        );
        self.effects.push(Effect::ClearProgress);
    }

    pub fn title(&self, catalog: &I18n) -> String {
        let Some(selection) = &self.selection else {
            return String::new();
        };
        let name = selection.files[0].path.rsplit('/').next().unwrap_or("");
        match &self.card {
            Some(Card::Combine(_)) => {
                catalog.trn("dialogs.output.title.combine", selection.files.len() as i64)
            }
            Some(Card::Print(_)) if selection.files.len() == 1 => {
                catalog.tr_args("dialogs.output.title.print", &[name])
            }
            Some(Card::Print(_)) => catalog.trn(
                "dialogs.output.title.print_many",
                selection.files.len() as i64,
            ),
            Some(Card::Convert(_)) if selection.files[0].kind == Kind::Office => {
                catalog.tr_args("dialogs.output.title.office", &[name])
            }
            Some(Card::Convert(_)) if selection.files.len() == 1 => {
                catalog.tr_args("dialogs.output.title.convert", &[name])
            }
            Some(Card::Convert(_)) => catalog.trn(
                "dialogs.output.title.convert_many",
                selection.files.len() as i64,
            ),
            None => String::new(),
        }
    }

    pub fn set_error(&mut self, catalog: &I18n, key: &'static str, args: &[&str]) {
        self.remember_error(catalog, key, args);
        self.effects.push(Effect::Status {
            error: true,
            text: self.error.clone(),
        });
    }

    pub(super) fn remember_error(&mut self, catalog: &I18n, key: &'static str, args: &[&str]) {
        self.error_spec = Some(ErrorText::Args(
            key,
            args.iter().map(|s| (*s).into()).collect(),
        ));
        self.error = catalog.tr_args(key, args);
    }

    pub fn clear_error(&mut self) {
        self.error.clear();
        self.error_spec = None;
    }

    pub fn set_count_error(&mut self, catalog: &I18n, key: &'static str, count: i64) {
        self.error_spec = Some(ErrorText::Count(key, count));
        self.error = catalog.trn(key, count);
        self.effects.push(Effect::Status {
            error: true,
            text: self.error.clone(),
        });
    }

    pub fn localize(&mut self, catalog: &I18n) {
        if let Some(spec) = &self.error_spec {
            self.error = match spec {
                ErrorText::Args(key, args) => {
                    catalog.tr_args(key, &args.iter().map(String::as_str).collect::<Vec<_>>())
                }
                ErrorText::Count(key, count) => catalog.trn(key, *count),
                ErrorText::Io(key, diagnostic) => {
                    let reason = key.map(|key| catalog.tr(key));
                    catalog.tr_args(
                        "dialogs.output.error.output",
                        &[reason.as_deref().unwrap_or(diagnostic)],
                    )
                }
            };
            self.effects.push(Effect::Status {
                error: true,
                text: self.error.clone(),
            });
        }
    }

    pub fn set_io_error(&mut self, catalog: &I18n, key: Option<&'static str>, diagnostic: String) {
        self.error_spec = Some(ErrorText::Io(key, diagnostic));
        self.localize(catalog);
    }

    pub fn focus_order(&self) -> Vec<Focus> {
        if self.unavailable {
            return vec![Focus::Cancel];
        }
        let mut order = match &self.card {
            Some(Card::Convert(c)) => match c.kind {
                Kind::Office => vec![],
                Kind::Pdf => vec![Focus::Formats, Focus::Pages],
                _ if c.check_enabled(self.selection.as_ref().map_or(0, |s| s.files.len())) => {
                    vec![Focus::Formats, Focus::Check]
                }
                _ => vec![Focus::Formats],
            },
            Some(Card::Print(p)) if p.destinations.is_empty() => vec![],
            Some(Card::Print(p)) => {
                let mut order = vec![Focus::Printer, Focus::Copies];
                if self
                    .selection
                    .as_ref()
                    .is_some_and(|s| s.files.iter().any(|f| f.kind.paged()))
                {
                    order.push(Focus::Pages);
                }
                if p.capabilities.duplex {
                    order.push(Focus::Duplex);
                }
                if p.capabilities.color {
                    order.push(Focus::Grayscale);
                }
                order
            }
            Some(Card::Combine(c)) => {
                let mut order = vec![Focus::Preview, Focus::Layout];
                if c.grid() {
                    order.push(Focus::Columns);
                }
                order.extend([Focus::Format, Focus::Spacing]);
                if c.background_enabled(self.selection.as_ref().map_or(0, |s| s.files.len())) {
                    order.push(Focus::Background);
                }
                order.push(Focus::Name);
                order
            }
            None => return vec![],
        };
        order.extend([Focus::Cancel, Focus::Submit]);
        order
    }

    pub fn tab(&mut self, reverse: bool) {
        if self.busy {
            return;
        }
        let order = self.focus_order();
        if order.is_empty() {
            return;
        }
        let at = order
            .iter()
            .position(|f| Some(*f) == self.focus)
            .unwrap_or(0);
        let at = if reverse {
            (at + order.len() - 1) % order.len()
        } else {
            (at + 1) % order.len()
        };
        self.focus = Some(order[at]);
    }
}
