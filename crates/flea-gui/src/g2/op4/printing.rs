//! Printer capabilities land only for the current destination request.
use super::{
    model::{Card, Model},
    seam::{Effect, Ticket},
};
use flea::{
    i18n::I18n,
    ui_logic_op4::printing::{Capabilities, Destination},
};

pub struct Print {
    pub destinations: Vec<Destination>,
    pub printer: String,
    pub printer_name: String,
    pub pending_printer: String,
    pub capabilities: Capabilities,
    pub copies: String,
    pub pages: String,
    pub two_sided: bool,
    pub grayscale: bool,
    pub pending: Option<Ticket>,
    pub setup: Vec<String>,
    pub queried: bool,
}

impl Default for Print {
    fn default() -> Self {
        Self::new()
    }
}
impl Print {
    pub fn new() -> Self {
        Self {
            destinations: vec![],
            printer: String::new(),
            printer_name: String::new(),
            pending_printer: String::new(),
            capabilities: Capabilities::default(),
            copies: "1".into(),
            pages: String::new(),
            two_sided: false,
            grayscale: false,
            pending: None,
            setup: vec![],
            queried: false,
        }
    }
}

impl Model {
    pub fn query_printer(&mut self, index: Option<usize>) {
        if !self.editable() {
            return;
        }
        self.clear_error();
        let ticket = self.ticket();
        let Some(Card::Print(p)) = &mut self.card else {
            return;
        };
        if let Some(index) = index {
            let Some(destination) = p.destinations.get(index) else {
                return;
            };
            p.pending_printer = destination.name.clone();
        } else {
            p.pending_printer = p.printer.clone();
        }
        p.pending = Some(ticket);
        self.effects.push(Effect::Send(super::wire::printer_query(
            ticket.request,
            &p.pending_printer,
        )));
    }

    pub fn printer_reply(&mut self, line: &flea::jsondoc::Json, catalog: &I18n) -> bool {
        use super::wire::{boolean, number, strings, text};
        let Some(Card::Print(p)) = &mut self.card else {
            return false;
        };
        let Some(ticket) = p.pending else {
            return false;
        };
        if number(line, "id") != Some(ticket.request) || self.unavailable {
            return false;
        }
        let Some(ok) = boolean(line, "ok") else {
            return false;
        };
        if !ok {
            p.pending = None;
            self.set_error(
                catalog,
                "dialogs.output.error.printer_query",
                &[text(line, "err").unwrap_or("")],
            );
            return true;
        }
        let Some(flea::jsondoc::Json::Arr(items)) = line.get("destinations") else {
            return false;
        };
        if items.len() > 256 {
            return false;
        }
        let destinations = items
            .iter()
            .map(|d| {
                Some(Destination {
                    name: text(d, "name")?.into(),
                    default: boolean(d, "default")?,
                })
            })
            .collect::<Option<Vec<_>>>();
        let Some(mut destinations) = destinations else {
            return false;
        };
        let Some(printer) = text(line, "printer") else {
            return false;
        };
        if !p.pending_printer.is_empty() && printer != p.pending_printer {
            return false;
        }
        if !destinations.is_empty() && !destinations.iter().any(|d| d.name == printer) {
            return false;
        }
        let (Some(duplex), Some(two_sided), Some(color), Some(grayscale)) = (
            boolean(line, "duplex"),
            boolean(line, "twoSided"),
            boolean(line, "color"),
            boolean(line, "grayscale"),
        ) else {
            return false;
        };
        destinations.sort_by_key(|d| !d.default);
        p.destinations = destinations;
        p.printer = printer.into();
        p.printer_name = text(line, "printerName")
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(printer)
            .into();
        p.capabilities = Capabilities {
            duplex,
            two_sided,
            color,
            grayscale,
        };
        p.two_sided = two_sided;
        p.grayscale = grayscale;
        p.setup = strings(line, "setupArgv").unwrap_or_default();
        p.pending = None;
        p.queried = true;
        true
    }
}
