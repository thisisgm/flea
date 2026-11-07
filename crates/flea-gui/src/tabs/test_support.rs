//! In-memory panes for the tab transfer regressions; no filesystem or native transport is involved.
use crate::{
    navigation::{folder_state::Order, history::Entry},
    tabs::{
        state::{Live, Open, Row, Say, TabState},
        Bar, Panes,
    },
};

pub struct Pane {
    pub path: String,
    pub busy: bool,
    pub view: &'static str,
    pub order: Order,
    pub selected: Vec<usize>,
    pub cursor: usize,
    pub requests: usize,
    pub search: bool,
    pub said: Vec<Say>,
}

impl Pane {
    pub fn new(path: &str) -> Self {
        Self {
            path: path.into(),
            busy: false,
            view: "list",
            order: Order::default(),
            selected: Vec::new(),
            cursor: 0,
            requests: 0,
            search: false,
            said: Vec::new(),
        }
    }
}

impl Live for Pane {
    fn path(&self) -> String {
        self.path.clone()
    }
    fn resting_path(&self) -> String {
        self.path.clone()
    }
    fn search_live(&self) -> bool {
        self.search
    }
    fn search_base(&self) -> String {
        self.path.clone()
    }
    fn busy(&self) -> bool {
        self.busy
    }
    fn drop_path(&self) -> String {
        String::new()
    }
    fn home(&self) -> String {
        "/files".into()
    }
    fn history(&self) -> (Vec<Entry>, Vec<Entry>) {
        Default::default()
    }
    fn set_history(&mut self, _: Vec<Entry>, _: Vec<Entry>) {}
    fn cursor(&self) -> usize {
        self.cursor
    }
    fn row(&self, _: usize) -> Option<Row> {
        None
    }
    fn held(&self) -> (usize, usize) {
        (0, 0)
    }
    fn total(&self) -> usize {
        0
    }
    fn window_size(&self) -> usize {
        40
    }
    fn view(&self) -> &'static str {
        self.view
    }
    fn adopt_view(&mut self, _: &str, view: &'static str) {
        self.view = view;
    }
    fn hidden(&self) -> bool {
        false
    }
    fn set_hidden(&mut self, _: bool) {}
    fn selected(&self) -> Vec<usize> {
        self.selected.clone()
    }
    fn follows(&self) -> bool {
        false
    }
    fn order(&self) -> Order {
        self.order
    }
    fn list_requests(&self) -> usize {
        self.requests
    }
    fn dev(&self) -> u64 {
        0
    }
    fn rename_pending(&self) -> bool {
        false
    }
    fn new_tab_path(&self, here: &str) -> String {
        here.into()
    }
    fn close_preview(&mut self) {}
    fn drop_overlay(&mut self) -> bool {
        false
    }
    fn open(&mut self, path: &str, open: Open) {
        self.path = path.into();
        self.busy = true;
        self.requests += 1;
        if let Some(order) = open.order {
            self.order = order;
        }
    }
    fn set_cursor(&mut self, index: usize) {
        self.cursor = index;
    }
    fn clear_selection(&mut self) {
        self.selected.clear();
    }
    fn select_only(&mut self, index: usize) {
        self.selected = vec![index];
    }
    // The live pane's toggle: a selected row drops, any other joins.
    fn toggle(&mut self, index: usize) {
        if let Some(at) = self.selected.iter().position(|row| *row == index) {
            self.selected.remove(at);
        } else {
            self.selected.push(index);
        }
    }
    fn selection_changed(&mut self) {}
    fn restore_order(&mut self, order: Order) {
        self.order = order;
    }
    fn sort(&mut self, order: Order) {
        self.order = order;
    }
    fn window(&mut self, _: usize, _: usize) {}
    fn can_locate(&self) -> bool {
        false
    }
    fn locate(&mut self, _: &str) {}
    fn peek(&mut self, _: &str, _: usize, _: bool, _: bool) {}
    fn say(&mut self, say: Say) -> u64 {
        self.said.push(say);
        self.said.len() as u64
    }
    fn withdraw(&mut self, _: u64) {}
}

pub struct World {
    pub tabs: TabState,
    pub pane: Pane,
}

impl World {
    pub fn new(count: usize) -> Self {
        let mut world = Self {
            tabs: TabState::default(),
            pane: Pane::new("/files"),
        };
        for i in 1..count {
            world
                .tabs
                .open_new(&mut world.pane, Some(format!("/files/{i}")));
            world.pane.busy = false;
        }
        world
    }

    pub fn bar(&self) -> Bar {
        let mut bar = Bar::new("111");
        bar.pane = Some(0);
        bar.width = 600.;
        bar
    }

    pub fn identities(&self) -> Vec<u64> {
        self.tabs
            .tabs
            .as_ref()
            .unwrap()
            .items
            .iter()
            .map(|tab| tab.identity)
            .collect()
    }
}

impl Panes for World {
    fn visit(&mut self, side: usize, f: &mut dyn FnMut(&mut TabState, &mut dyn Live)) -> bool {
        if side != 0 {
            return false;
        }
        f(&mut self.tabs, &mut self.pane);
        true
    }
}
