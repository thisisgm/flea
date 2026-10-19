use super::locale::Snapshot;
use crate::{
    backend::{Backend, Failure},
    list::{pane::Pane, ListState, Typeface},
    navigation::Navigator,
    row::format::Words,
};
use gpui::{SharedString, Task};
use std::{cell::RefCell, rc::Rc};

pub(crate) struct PaneHost {
    pub path: String,
    pub pane: Pane,
    pub list: Rc<RefCell<ListState>>,
    pub backend: Option<Backend>,
    pub reader: Option<Task<()>>,
    pub transition: Option<Task<()>>,
    pub selected: Vec<usize>,
    pub select: Option<String>,
    pub error: Option<String>,
    pub failure: Option<Failure>,
    pub status: SharedString,
    pub state_text: crate::pane::states::Text,
    pub answered: bool,
    pub waiting: bool,
    pub stopped: bool,
    pub nav: Navigator,
    pub watch_task: Option<(u64, Task<()>)>,
    applied_listing: usize,
}

impl PaneHost {
    pub fn new(path: String, select: Option<String>, height: f32, locale: &Snapshot) -> Self {
        Self {
            pane: Pane::new(path.clone(), height),
            path,
            select,
            list: Rc::new(RefCell::new(ListState::with_words(
                Typeface::omarchy_default(),
                Words::from_catalog(&locale.catalog),
            ))),
            backend: None,
            reader: None,
            transition: None,
            selected: Vec::new(),
            error: None,
            failure: None,
            status: locale.catalog.tr("window.loading").into(),
            state_text: crate::pane::states::State::default().text(&locale.catalog, 0),
            answered: false,
            waiting: false,
            stopped: false,
            nav: Navigator::new(Rc::default(), Rc::default()),
            watch_task: None,
            applied_listing: 0,
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
        self.list
            .borrow_mut()
            .set_words(Words::from_catalog(&locale.catalog));
        self.state_text = self.pane.state.text(&locale.catalog, self.pane.held.total);
        if let Some(failure) = self.failure.clone() {
            self.record_error(failure, locale);
        } else {
            self.status =
                if self.answered && self.pane.state.kind == crate::pane::states::Kind::Ready {
                    locale
                        .catalog
                        .trn("items.count", self.pane.held.total as i64)
                } else if self.waiting {
                    locale.catalog.tr("window.waiting")
                } else {
                    self.state_text.status.clone()
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

    pub fn changed(&mut self, locale: &Snapshot) {
        let generation = self.pane.held.listing;
        if generation != self.applied_listing {
            self.selected.clear();
            self.applied_listing = generation;
        }
        self.path = self.pane.path().into();
        self.answered = !self.pane.busy();
        self.error = None;
        self.failure = None;
        self.state_text = self.pane.state.text(&locale.catalog, self.pane.held.total);
        self.status = self.state_text.status.clone().into();
        if self.answered {
            self.apply_select();
        }
    }
}
