//! Conversion drafts and the off-thread output-existence seam.
use super::{
    model::{Card, Focus, Model},
    seam::{Effect, OutputError, Ticket},
};
use flea::{
    i18n::I18n,
    ui_logic_op4::{
        output::Kind,
        pages::Pages,
        selection::{default_format, Selection},
    },
};

pub struct Convert {
    pub cursor: usize,
    pub kind: Kind,
    pub format: String,
    pub strip: bool,
    pub combine: bool,
    pub pages: String,
    pub page_count: Option<u32>,
    pub pending: Option<Ticket>,
    pub count_pending: Option<Ticket>,
    pub outputs: Vec<String>,
    pub collision: bool,
}

impl Convert {
    pub fn new(selection: &Selection) -> Self {
        let format = default_format(&selection.files[0]);
        Self {
            cursor: selection.files[0]
                .kind
                .formats()
                .iter()
                .position(|f| *f == format)
                .unwrap_or(0),
            kind: selection.files[0].kind,
            format: default_format(&selection.files[0]).into(),
            strip: false,
            combine: true,
            pages: String::new(),
            page_count: None,
            pending: None,
            count_pending: None,
            outputs: vec![],
            collision: false,
        }
    }
    pub fn check_enabled(&self, count: usize) -> bool {
        self.kind == Kind::Image && (self.format != "pdf" || count > 1)
    }
}

impl Model {
    pub fn choose_format(&mut self, index: usize, catalog: &I18n) {
        if !self.editable() {
            return;
        }
        let Some(Card::Convert(c)) = &mut self.card else {
            return;
        };
        let Some(format) = c.kind.formats().get(index) else {
            return;
        };
        c.format = (*format).into();
        c.cursor = index;
        self.focus = Some(Focus::Formats);
        self.probe(catalog);
    }

    pub fn toggle_check(&mut self, catalog: &I18n) {
        if !self.editable() {
            return;
        }
        let Some(Card::Convert(c)) = &mut self.card else {
            return;
        };
        if c.kind != Kind::Image {
            return;
        }
        if c.format == "pdf" {
            if self.selection.as_ref().is_some_and(|s| s.files.len() > 1) {
                c.combine = !c.combine;
            } else {
                return;
            }
        } else {
            c.strip = !c.strip;
            self.focus = Some(Focus::Check);
            return;
        }
        self.focus = Some(Focus::Check);
        self.probe(catalog);
    }

    pub fn probe(&mut self, catalog: &I18n) {
        if !self.editable() {
            return;
        }
        self.clear_error();
        let ticket = self.ticket();
        let Some(selection) = &self.selection else {
            return;
        };
        let paths = match &mut self.card {
            Some(Card::Convert(c)) => {
                c.pending = None;
                c.collision = false;
                if c.kind == Kind::Pdf && c.page_count.is_none() {
                    if c.count_pending.is_none() {
                        c.count_pending = Some(ticket);
                        self.effects.push(Effect::DocumentPages {
                            ticket,
                            path: selection.files[0].path.clone(),
                        });
                    }
                    return;
                }
                let pages = match Pages::parse(&c.pages) {
                    Ok(pages) => pages,
                    Err(_) => {
                        self.set_error(catalog, "dialogs.output.error.pages", &[]);
                        return;
                    }
                };
                if let (Pages::Ranges(ranges), Some(count)) = (&pages, c.page_count) {
                    if ranges.iter().any(|(_, end)| *end > count) {
                        self.set_count_error(
                            catalog,
                            "dialogs.output.error.document_pages",
                            i64::from(count),
                        );
                        return;
                    }
                }
                match selection.output_paths(&c.format, c.combine, &pages, c.page_count) {
                    Ok(paths) => {
                        c.outputs = paths
                            .iter()
                            .map(|p| p.to_string_lossy().into_owned())
                            .collect();
                        c.pending = Some(ticket);
                        c.outputs.clone()
                    }
                    Err(key) => {
                        self.set_error(catalog, key, &[]);
                        return;
                    }
                }
            }
            Some(Card::Combine(c)) => match c.output(selection) {
                Ok(path) => {
                    c.pending = Some(ticket);
                    c.collision = false;
                    vec![path]
                }
                Err(key) => {
                    self.set_error(catalog, key, &[]);
                    return;
                }
            },
            _ => return,
        };
        self.effects.push(Effect::CheckOutputs { ticket, paths });
    }

    pub fn document_pages(&mut self, ticket: Ticket, count: Result<u32, String>, catalog: &I18n) {
        let Some(Card::Convert(c)) = &mut self.card else {
            return;
        };
        if c.count_pending != Some(ticket) || self.unavailable {
            return;
        }
        c.count_pending = None;
        match count {
            Ok(n) if (1..=10_000).contains(&n) => {
                c.page_count = Some(n);
                self.probe(catalog);
            }
            Ok(_) => self.set_error(catalog, "dialogs.output.error.pdf_limit", &[]),
            Err(reason) => self.set_error(
                catalog,
                "dialogs.output.error.page_count_failed",
                &[&reason],
            ),
        }
    }

    pub fn checked(&mut self, ticket: Ticket, result: Result<(), OutputError>, catalog: &I18n) {
        if !self.editable() {
            return;
        }
        let pending = match &mut self.card {
            Some(Card::Convert(c)) => (&mut c.pending, &mut c.collision),
            Some(Card::Combine(c)) => (&mut c.pending, &mut c.collision),
            _ => return,
        };
        if *pending.0 != Some(ticket) {
            return;
        }
        *pending.0 = None;
        *pending.1 = result.is_err();
        match result {
            Ok(()) => self.clear_error(),
            Err(OutputError::Exists(name)) => {
                self.set_error(catalog, "dialogs.output.error.exists", &[&name])
            }
            Err(OutputError::Access { key, diagnostic }) => {
                self.set_io_error(catalog, key, diagnostic)
            }
        }
    }

    pub fn checked_paths(
        &mut self,
        ticket: Ticket,
        result: Result<Vec<String>, OutputError>,
        catalog: &I18n,
    ) {
        if !self.editable() {
            return;
        }
        if let Some(Card::Convert(c)) = &mut self.card {
            if c.pending != Some(ticket) {
                return;
            }
            if let Ok(paths) = &result {
                c.outputs = paths.clone();
            }
        }
        self.checked(ticket, result.map(|_| ()), catalog);
    }
}
