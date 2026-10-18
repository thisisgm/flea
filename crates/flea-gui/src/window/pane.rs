use super::locale::Snapshot;
use crate::{
    backend::{Backend, Failure},
    list::{pane::Pane, ListState, Typeface},
};
use gpui::{SharedString, Task};
use std::{cell::RefCell, rc::Rc};

pub(crate) struct PaneHost {
    pub path: String,
    pub pane: Pane,
    pub list: Rc<RefCell<ListState>>,
    pub backend: Option<Backend>,
    pub reader: Option<Task<()>>,
    pub selected: Vec<usize>,
    pub select: Option<String>,
    pub error: Option<String>,
    pub failure: Option<Failure>,
    pub status: SharedString,
    pub answered: bool,
    pub waiting: bool,
    pub stopped: bool,
}

impl PaneHost {
    pub fn new(path: String, select: Option<String>, height: f32, locale: &Snapshot) -> Self {
        Self {
            pane: Pane::new(path.clone(), height),
            path,
            select,
            list: Rc::new(RefCell::new(ListState::new(Typeface::omarchy_default()))),
            backend: None,
            reader: None,
            selected: Vec::new(),
            error: None,
            failure: None,
            status: locale.catalog.tr("window.loading").into(),
            answered: false,
            waiting: false,
            stopped: false,
        }
    }

    pub fn record_error(&mut self, error: Failure, locale: &Snapshot) {
        let message = locale.catalog.tr_args(error.key, &[&error.detail]);
        if self.error.as_ref() != Some(&message) {
            let diagnostic = message.clone();
            let _ = std::thread::Builder::new()
                .name("flea-diagnostic".into())
                .spawn(move || eprintln!("flea-gui: {diagnostic}"));
        }
        self.failure = Some(error);
        self.error = Some(message.clone());
        self.status = message.into();
    }

    pub fn translated(&mut self, locale: &Snapshot) {
        self.list.borrow_mut().invalidate_text();
        if let Some(failure) = self.failure.clone() {
            self.record_error(failure, locale);
        } else {
            self.status = if self.answered {
                locale
                    .catalog
                    .trn("items.count", self.pane.held.total as i64)
            } else if self.waiting {
                locale.catalog.tr("window.waiting")
            } else {
                locale.catalog.tr("window.loading")
            }
            .into();
        }
    }

    pub fn apply_select(&mut self) {
        let Some(target) = self.select.take() else {
            return;
        };
        if std::path::Path::new(&target).parent() != Some(std::path::Path::new(&self.path)) {
            return;
        }
        let Some(name) = std::path::Path::new(&target)
            .file_name()
            .and_then(|name| name.to_str())
        else {
            return;
        };
        if let Some(at) = self.pane.held.rows.iter().position(|row| row.name == name) {
            self.pane.cursor = self.pane.held.start + at;
            self.pane.reveal_cursor();
        }
    }
}
