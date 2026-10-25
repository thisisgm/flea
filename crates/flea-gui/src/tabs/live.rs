//! The tab model's live pane adapter, using the window's navigator and bounded held rows.
use super::state::{Live, Open, Row, Say};
use crate::{
    navigation::{folder_state::Order, history::Entry, Command},
    pane::swap::Options,
    window::{
        pane::PaneHost,
        startup::{Settings, View},
    },
};
use flea::i18n::I18n;

#[derive(Default)]
pub struct Notice {
    pub serial: u64,
    pub text: String,
    // What the footer still owes: a sentence to say, a withdrawal, and the footer serial now standing.
    pub owed: Option<String>,
    pub withdraw: bool,
    pub shown: Option<u64>,
}

pub(crate) struct Pane<'a> {
    pub host: &'a mut PaneHost,
    pub view: &'a mut View,
    pub settings: &'a Settings,
    pub home: &'a str,
    pub catalog: &'a I18n,
    pub notice: &'a mut Notice,
}

pub fn view_name(view: View) -> &'static str {
    match view {
        View::List => "list",
        View::Columns => "columns",
        View::Grid => "grid",
        View::Dual => "dual",
    }
}

impl Live for Pane<'_> {
    fn path(&self) -> String {
        self.host.pane.path().into()
    }
    fn resting_path(&self) -> String {
        self.host.pane.place().into()
    }
    fn search_live(&self) -> bool {
        self.host.pane.state.search
    }
    fn search_base(&self) -> String {
        self.path()
    }
    fn busy(&self) -> bool {
        self.host.stopped || self.host.pane.busy()
    }
    fn drop_path(&self) -> String {
        self.resting_path()
    }
    fn home(&self) -> String {
        self.home.into()
    }
    fn history(&self) -> (Vec<Entry>, Vec<Entry>) {
        (
            self.host.nav.history.back.clone(),
            self.host.nav.history.forward.clone(),
        )
    }
    fn set_history(&mut self, back: Vec<Entry>, forward: Vec<Entry>) {
        self.host.nav.history.back = back;
        self.host.nav.history.forward = forward;
    }
    fn cursor(&self) -> usize {
        self.host.pane.cursor
    }
    fn row(&self, index: usize) -> Option<Row> {
        self.host.pane.held.row(index).map(|row| Row {
            name: row.name.clone(),
            dir: row.dir,
        })
    }
    fn held(&self) -> (usize, usize) {
        (self.host.pane.held.start, self.host.pane.held.rows.len())
    }
    fn total(&self) -> usize {
        self.host.pane.held.total
    }
    fn window_size(&self) -> usize {
        self.host.pane.first_count()
    }
    fn view(&self) -> &'static str {
        view_name(*self.view)
    }
    fn adopt_view(&mut self, path: &str, view: &'static str) {
        if *self.view == View::Dual {
            return;
        }
        let view = self.host.nav.folders.borrow().view_for(path, view);
        *self.view = match view {
            "grid" => View::Grid,
            "columns" => View::Columns,
            _ => View::List,
        };
    }
    fn hidden(&self) -> bool {
        self.host.nav.folders.borrow().hidden
    }
    fn set_hidden(&mut self, hidden: bool) {
        self.host.nav.folders.borrow_mut().hidden = hidden;
    }
    fn selected(&self) -> Vec<usize> {
        self.host.selected.clone()
    }
    fn follows(&self) -> bool {
        self.host.selected == [self.host.pane.cursor]
    }
    fn order(&self) -> Order {
        self.host.nav.order
    }
    fn sort_snapshot(&self) -> Option<crate::navigation::sort::Snapshot> {
        if self.host.nav.pending_sort.is_some() {
            return None;
        }
        self.host.nav.sort_snapshot(&self.host.pane).ok()
    }
    fn list_requests(&self) -> usize {
        self.host.pane.list_requests
    }
    // RowFacts carries no device id; unknown keeps the transfer policy conservative.
    fn dev(&self) -> u64 {
        0
    }
    fn rename_pending(&self) -> bool {
        self.host.nav.gate.editing || self.host.nav.gate.rename_pending
    }
    fn new_tab_path(&self, here: &str) -> String {
        self.settings.new_tab_path(here, self.home)
    }
    // The preview owner subscribes to the tab action before a new listing replaces its row.
    fn close_preview(&mut self) {}
    fn drop_overlay(&mut self) -> bool {
        let search = self.host.pane.state.search;
        if search {
            self.host.pane.swap.session.leaving = true;
            self.host.pane.state.search = false;
            self.host.pane.state.query.clear();
        }
        search
    }
    fn open(&mut self, path: &str, open: Open) {
        if let Some(order) = open.order {
            let _ = self.host.nav.restore_sort_snapshot(
                &mut self.host.pane,
                &crate::navigation::sort::Snapshot {
                    path: path.into(),
                    order,
                },
            );
            return;
        }
        let _ = self.host.nav.open_without_history(
            &mut self.host.pane,
            path,
            Options {
                supersede: open.supersede,
                clear_at_once: open.clear_at_once,
                ..Options::default()
            },
            None,
            false,
        );
    }
    fn set_cursor(&mut self, index: usize) {
        self.host.pane.cursor = index.min(self.total().saturating_sub(1));
        self.host.pane.reveal_cursor();
    }
    fn clear_selection(&mut self) {
        self.host.selected.clear();
    }
    fn select_only(&mut self, index: usize) {
        self.host.selected = vec![index];
    }
    fn toggle(&mut self, index: usize) {
        if let Some(at) = self.host.selected.iter().position(|row| *row == index) {
            self.host.selected.remove(at);
        } else {
            self.host.selected.push(index);
        }
    }
    fn selection_changed(&mut self) {
        self.host.selected.sort_unstable();
    }
    fn resort(&mut self, order: Order) {
        let _ = self.host.nav.resort(
            &mut self.host.pane,
            order,
            &mut self.host.selected,
            self.catalog.tr("navigation.save_failed"),
        );
    }
    fn sort(&mut self, order: Order) {
        self.host.nav.order = order;
        let folders = self.host.nav.folders.borrow();
        self.host.nav.counters.sorts += 1;
        self.host.nav.out.push(Command::Sort {
            order,
            folders_first: folders.folders_first,
            group_by_kind: folders.group_by_kind,
            hidden_last: folders.hidden_last,
        });
        drop(folders);
        self.host.selected.clear();
        self.host.pane.cursor = 0;
        self.host.pane.scroll_to(0.);
        let count = self.window_size();
        self.host.pane.swap.sorts_out.push_back(count);
        self.window(0, count);
    }
    fn window(&mut self, start: usize, count: usize) {
        self.host.nav.counters.windows += 1;
        self.host.nav.out.push(Command::Window { start, count });
    }
    fn can_locate(&self) -> bool {
        !self.host.stopped && self.host.backend.is_some()
    }
    fn locate(&mut self, path: &str) {
        self.host.nav.counters.locates += 1;
        self.host
            .nav
            .out
            .push(Command::Locate { path: path.into() });
    }
    fn peek(&mut self, path: &str, first: usize, hidden: bool, hidden_last: bool) {
        if let Some(backend) = &self.host.backend {
            if backend
                .line(super::transfer::peek_line(path, first, hidden, hidden_last))
                .is_err()
            {
                self.say(Say::NotReceived);
            }
        }
    }
    fn say(&mut self, say: Say) -> u64 {
        self.notice.serial += 1;
        self.notice.text = say.text(self.catalog);
        self.notice.owed = Some(self.notice.text.clone());
        self.host.status = self.notice.text.clone().into();
        self.notice.serial
    }
    fn withdraw(&mut self, serial: u64) {
        if self.notice.serial == serial {
            if self.host.status.as_ref() == self.notice.text {
                self.host.status = self.host.state_text.status.clone().into();
            }
            self.notice.text.clear();
            self.notice.owed = None;
            self.notice.withdraw = true;
        }
    }
}
