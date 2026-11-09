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
    pub outline_request: Option<super::outline::Request>,
    pub outline_used: bool,
    /// The listing an in-place open or close left while its one mark was a lone mark the cursor carries.
    pub outline_lone: Option<usize>,
    pub git: super::git_status::Runtime,
    pub recent: flea::ui_logic_rail_places::recentmode::RecentMode,
    pub search: super::search::Runtime,
    pub look_warm: super::quick_look::warm::Warm,
    pub filter: super::filter::Runtime,
    pub preview: Option<gpui::Entity<super::selection_preview::SelectionPreview>>,
    pub preview_port: super::selection_preview::Port,
    pub preview_watch: Option<gpui::Subscription>,
    pub columns: Option<super::columns::Columns>,
    pub path: String,
    pub pane: Pane,
    pub list: Rc<RefCell<ListState>>,
    pub drawn_view: Option<super::startup::View>,
    pub drawn_list: bool,
    pub grid: Option<crate::grid::host::GridHost>,
    pub backend: Option<Backend>,
    pub reader: Option<Task<()>>,
    pub transition: Option<Task<()>>,
    pub transition_at: Option<std::time::Instant>,
    /// Ascending, unique row indices; producers normalize once when the selection changes.
    pub selected: Vec<usize>,
    pub sizes: crate::list::sizes::State,
    pub input: crate::input::PaneInput,
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
    pub retry_line: RefCell<Option<super::retry::RetryLine>>,
    // The operation line last written to status; a listing that lands after it keeps it, as ui/StatusBar.qml keeps its notice.
    pub operation_line: Option<String>,
    applied_listing: usize,
}

impl PaneHost {
    #[cfg(test)]
    pub fn new(path: String, select: Option<String>, height: f32, locale: &Snapshot) -> Self {
        Self::with_typeface(path, select, height, locale, Typeface::omarchy_default())
    }

    pub fn with_typeface(
        path: String,
        select: Option<String>,
        height: f32,
        locale: &Snapshot,
        typeface: Typeface,
    ) -> Self {
        Self {
            outline_request: None,
            outline_used: false,
            outline_lone: None,
            git: Default::default(),
            recent: Default::default(),
            search: super::search::Runtime::new(typeface.clone()),
            look_warm: Default::default(),
            filter: super::filter::Runtime::new(typeface.clone()),
            preview: None,
            preview_port: Default::default(),
            preview_watch: None,
            columns: None,
            pane: Pane::at_row_height(
                path.clone(),
                height,
                crate::list::Metrics::for_typeface(&typeface).row,
            ),
            path,
            select,
            list: Rc::new(RefCell::new(ListState::with_words(
                typeface,
                Words::from_catalog(&locale.catalog),
            ))),
            drawn_view: None,
            drawn_list: false,
            grid: None,
            backend: None,
            reader: None,
            transition: None,
            transition_at: None,
            selected: Vec::new(),
            sizes: Default::default(),
            input: Default::default(),
            error: None,
            failure: None,
            status: locale.catalog.tr("window.loading").into(),
            state_text: crate::pane::states::State::default().text(&locale.catalog, 0),
            answered: false,
            waiting: false,
            stopped: false,
            nav: Navigator::new(Rc::default(), Rc::default()),
            watch_task: None,
            retry_line: RefCell::new(None),
            operation_line: None,
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
        self.operation_line = None;
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
        if let Some(backend) = &self.backend {
            if let Some(request) = self.pane.reread_held() {
                if let Err(error) = backend.window(request.start, request.count) {
                    self.pane.requests.outstanding = None;
                    self.record_error(error, locale);
                }
            }
        }
    }

    /// The notice that carried the operation line timed out; a status still showing it returns to the folder's count.
    pub fn end_operation_line(&mut self) {
        if let Some(line) = self.operation_line.take() {
            if self.status.as_ref() == line {
                self.status = self.state_text.status.clone().into();
            }
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
            self.selected.clear();
            self.selected.push(self.pane.cursor);
        }
    }

    /// The re-read's lone mark, taken as a lone mark again so a plain move carries it.
    pub(crate) fn take_lone_mark(&mut self) {
        if let Some(row) = self.nav.lone_mark.take() {
            if self.selected == [row] {
                self.input.selection.only(&mut self.selected, row);
            }
        }
    }

    pub fn changed(&mut self, locale: &Snapshot) {
        let generation = self.pane.held.listing;
        let before = std::mem::take(&mut self.path);
        if generation != self.applied_listing {
            if before == self.pane.path()
                && self.pane.swap.retained_listing == Some(self.applied_listing)
            {
                self.list.borrow_mut().rows.reindex(&self.pane.held);
            }
            let keep_all = before == self.pane.path()
                && self.nav.sort_landing(&self.pane)
                && self.selected.len() == self.pane.held.total
                && self.selected.iter().copied().eq(0..self.pane.held.total);
            // Anchor.js anchoredRefresh: only a mark that follows the cursor comes back lone.
            if let Some(anchor) = self.nav.anchor.as_mut() {
                anchor.lone = anchor.lone && self.input.selection.is_lone();
            }
            // A replacement listing cannot restore the previous band's cursor.
            self.input.band = None;
            self.input.cancel(&mut self.selected, &mut self.pane.cursor);
            self.input.selection.reset();
            self.input.pulled = None;
            self.input.press = None;
            if keep_all {
                self.input.selection.changed(&self.selected);
            } else {
                self.selected.clear();
            }
            self.applied_listing = generation;
            self.filter_follow(&before);
        }
        self.path = self.pane.path().into();
        // Leaving the folder drops the operation line that was written there.
        if self.path != before {
            self.operation_line = None;
        }
        self.answered = !self.pane.busy();
        self.error = None;
        self.failure = None;
        self.state_text = self.pane.state.text(&locale.catalog, self.pane.held.total);
        // The completion's line holds across its own in-place refresh; any other status replaces it.
        if self.operation_line.as_deref() != Some(self.status.as_ref()) {
            self.operation_line = None;
            self.status = self.state_text.status.clone().into();
        }
        if self.answered {
            self.apply_select();
        }
    }
}

#[cfg(test)]
#[path = "pane_generation_cursor_tests.rs"]
mod generation_cursor_tests;
