use flea::i18n::{I18n, Paths};
use flea_gui::navigation::{folder_state::Order, history::Entry};
use flea_gui::tabs::{
    bar::Panes,
    state::{Live, Open, Row, Say},
    Bar, TabState,
};
use std::{path::PathBuf, process::Command as Process, sync::OnceLock};

// Every timed step runs on this clock, so an ack window is a sum rather than a sleep.
pub const NOW: u64 = 1_000_000;

pub fn root() -> PathBuf {
    let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.superpowers/g1-10");
    std::fs::create_dir_all(&parent).unwrap();
    let output = Process::new("mktemp")
        .arg("-d")
        .arg(parent.join("test.XXXXXXXX"))
        .output()
        .unwrap();
    assert!(output.status.success());
    let root = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
    assert!(root.is_absolute());
    std::fs::write(root.join(".flea-test-sandbox"), "g1-10\n").unwrap();
    root
}

// English from the embedded catalog only, never a catalog installed on the host.
pub fn english() -> I18n {
    static CATALOG: OnceLock<I18n> = OnceLock::new();
    CATALOG
        .get_or_init(|| {
            let root = root();
            std::fs::create_dir_all(root.join("omarchy/default/i18n")).unwrap();
            std::fs::write(
                root.join("omarchy/default/i18n/locales.json"),
                r#"{"en":{}}"#,
            )
            .unwrap();
            std::fs::write(root.join("language"), "en").unwrap();
            I18n::load(Paths {
                omarchy: root.join("omarchy"),
                language_file: root.join("language"),
                catalogs: vec![root.join("catalog")],
            })
        })
        .clone()
}

pub fn order(key: &'static str, desc: bool) -> Order {
    Order { key, desc }
}

pub fn row(name: &str, dir: bool) -> Option<Row> {
    Some(Row {
        name: name.into(),
        dir,
    })
}

pub type Rows = Box<dyn Fn(usize) -> Option<Row>>;
pub type OpenHook = fn(&mut Fake, &str, Open);

// The stub pane every tab check drives, the same defaults the shipped suites' fixture starts from.
pub struct Fake {
    pub path: String,
    pub listing_path: String,
    pub home: String,
    pub back: Vec<Entry>,
    pub forward: Vec<Entry>,
    pub cursor: usize,
    pub view: &'static str,
    pub hidden: bool,
    // The standing dotfile preference, not this tab's.
    pub preference_hidden: bool,
    pub search_mode: &'static str,
    pub search_query: String,
    pub search_from: String,
    pub search_base: String,
    pub search_running: bool,
    pub search_gen: u64,
    pub search_opened_gen: u64,
    pub search_leaving: bool,
    pub recent_mode: &'static str,
    pub recent_from: String,
    pub filter_query: String,
    pub busy: bool,
    pub listing_state: &'static str,
    pub drop_path: String,
    pub total: usize,
    pub window_size: usize,
    pub rows: Option<Rows>,
    pub held_first: usize,
    pub held_count: Option<usize>,
    pub order: Order,
    pub list_requests: usize,
    pub dev: u64,
    pub rename_pending: bool,
    pub preview_active: bool,
    pub preview_closed: usize,
    pub selection: Vec<usize>,
    pub follows: bool,
    // Bumped by every announced selection change, never by a bare clear.
    pub announced: usize,
    pub thumbs_stale: bool,
    pub sizes_stale: bool,
    pub can_locate: bool,
    pub said: Vec<String>,
    pub listed: Vec<String>,
    pub opened: Vec<Open>,
    pub sorted: Vec<String>,
    pub windows: Vec<String>,
    pub located: Vec<String>,
    pub peeks: Vec<(String, usize, bool, bool)>,
    pub adopted: Vec<(String, &'static str)>,
    pub cleared_at_once: bool,
    pub superseded: bool,
    pub notice: String,
    pub notice_serial: u64,
    pub errors: Vec<String>,
    pub on_open: Option<OpenHook>,
    catalog: I18n,
}

impl Fake {
    pub fn at(path: &str) -> Self {
        Self {
            path: path.into(),
            listing_path: String::new(),
            home: "/home/gm".into(),
            back: vec![Entry::new("/home/gm", "", 0)],
            forward: Vec::new(),
            cursor: 4,
            view: "list",
            hidden: false,
            preference_hidden: false,
            search_mode: "",
            search_query: String::new(),
            search_from: String::new(),
            search_base: String::new(),
            search_running: false,
            search_gen: 0,
            search_opened_gen: 0,
            search_leaving: false,
            recent_mode: "",
            recent_from: String::new(),
            filter_query: String::new(),
            busy: false,
            listing_state: "ready",
            drop_path: String::new(),
            total: 20,
            window_size: 40,
            rows: None,
            held_first: 0,
            held_count: None,
            order: Order::default(),
            list_requests: 0,
            dev: 0,
            rename_pending: false,
            preview_active: false,
            preview_closed: 0,
            selection: Vec::new(),
            follows: false,
            announced: 0,
            thumbs_stale: false,
            sizes_stale: false,
            can_locate: false,
            said: Vec::new(),
            listed: Vec::new(),
            opened: Vec::new(),
            sorted: Vec::new(),
            windows: Vec::new(),
            located: Vec::new(),
            peeks: Vec::new(),
            adopted: Vec::new(),
            cleared_at_once: false,
            superseded: false,
            notice: String::new(),
            notice_serial: 0,
            errors: Vec::new(),
            on_open: None,
            catalog: english(),
        }
    }

    pub fn standard() -> Self {
        Self::at("/home/gm/Work")
    }

    pub fn with_rows(mut self, rows: impl Fn(usize) -> Option<Row> + 'static) -> Self {
        self.rows = Some(Box::new(rows));
        self
    }

    pub fn toggled(mut self, rows: &[usize]) -> Self {
        for &row in rows {
            self.toggle(row);
        }
        self
    }

    // A message that is not the strip's own: a plain notice bumps the serial, an error stands apart.
    pub fn message(&mut self, text: &str, error: bool) {
        self.said.push(text.into());
        if error {
            self.errors.push(text.into());
        } else {
            self.notice = text.into();
            self.notice_serial += 1;
        }
    }

    pub fn last_said(&self) -> String {
        self.said.last().cloned().unwrap_or_default()
    }

    fn close_session(&mut self) {
        self.search_mode = "";
        self.search_query.clear();
        self.search_running = false;
        self.search_from.clear();
        self.search_base.clear();
    }

    // The fixture's openWithoutHistory: a listing forgets the selection and takes the dotfile answer unless told to keep it.
    fn list(&mut self, path: &str, open: Open) {
        if self.listing_state == "waiting" {
            self.busy = false;
            self.listing_state = "loading";
        }
        if !open.keep_hidden {
            self.hidden = self.preference_hidden;
        }
        self.cleared_at_once = open.clear_at_once;
        self.superseded = open.supersede;
        self.listed.push(path.into());
        self.opened.push(open);
        self.listing_path = path.into();
        self.path = path.into();
        self.cursor = 0;
        self.selection.clear();
        self.list_requests += 1;
    }
}

impl Live for Fake {
    fn path(&self) -> String {
        self.path.clone()
    }
    fn resting_path(&self) -> String {
        if self.search_mode == "results" && !self.search_from.is_empty() {
            return self.search_from.clone();
        }
        if self.recent_mode == "results" && !self.recent_from.is_empty() {
            return self.recent_from.clone();
        }
        if self.busy && !self.listing_path.is_empty() {
            self.listing_path.clone()
        } else {
            self.path.clone()
        }
    }
    fn search_live(&self) -> bool {
        !self.search_mode.is_empty() && !self.search_query.is_empty()
    }
    fn search_base(&self) -> String {
        if self.search_base.is_empty() {
            self.path.clone()
        } else {
            self.search_base.clone()
        }
    }
    fn busy(&self) -> bool {
        self.busy
    }
    fn drop_path(&self) -> String {
        self.drop_path.clone()
    }
    fn home(&self) -> String {
        self.home.clone()
    }
    fn history(&self) -> (Vec<Entry>, Vec<Entry>) {
        (self.back.clone(), self.forward.clone())
    }
    fn set_history(&mut self, back: Vec<Entry>, forward: Vec<Entry>) {
        self.back = back;
        self.forward = forward;
    }
    fn cursor(&self) -> usize {
        self.cursor
    }
    fn row(&self, index: usize) -> Option<Row> {
        self.rows.as_ref().and_then(|rows| rows(index))
    }
    fn held(&self) -> (usize, usize) {
        (self.held_first, self.held_count.unwrap_or(self.window_size))
    }
    fn total(&self) -> usize {
        self.total
    }
    fn window_size(&self) -> usize {
        self.window_size
    }
    fn view(&self) -> &'static str {
        self.view
    }
    fn adopt_view(&mut self, path: &str, view: &'static str) {
        self.view = view;
        self.adopted.push((path.into(), view));
    }
    fn hidden(&self) -> bool {
        self.hidden
    }
    fn set_hidden(&mut self, hidden: bool) {
        self.hidden = hidden;
    }
    fn selected(&self) -> Vec<usize> {
        self.selection.clone()
    }
    fn follows(&self) -> bool {
        self.follows
    }
    fn order(&self) -> Order {
        self.order
    }
    fn list_requests(&self) -> usize {
        self.list_requests
    }
    fn dev(&self) -> u64 {
        self.dev
    }
    fn rename_pending(&self) -> bool {
        self.rename_pending
    }
    fn new_tab_path(&self, here: &str) -> String {
        here.into()
    }
    fn close_preview(&mut self) {
        if self.preview_active {
            self.preview_active = false;
            self.preview_closed += 1;
        }
    }
    // The shipped order: a live search, then one owing a re-list, then a history; the walk and the filter close after.
    fn drop_overlay(&mut self) -> bool {
        let live = self.search_live();
        let owes = !self.search_mode.is_empty() && self.search_gen > self.search_opened_gen;
        let dropped = live
            || owes
            || if self.recent_mode.is_empty() {
                false
            } else {
                self.recent_mode = "";
                self.recent_from.clear();
                true
            };
        if !self.search_mode.is_empty() && !self.search_leaving {
            if live {
                self.search_leaving = true;
            } else {
                self.close_session();
            }
        }
        self.filter_query.clear();
        dropped
    }
    fn open(&mut self, path: &str, open: Open) {
        match self.on_open {
            Some(hook) => hook(self, path, open),
            None => self.list(path, open),
        }
    }
    fn set_cursor(&mut self, index: usize) {
        self.cursor = index;
    }
    fn clear_selection(&mut self) {
        self.selection.clear();
    }
    fn select_only(&mut self, index: usize) {
        self.selection = vec![index];
        self.follows = true;
    }
    fn toggle(&mut self, index: usize) {
        match self.selection.binary_search(&index) {
            Ok(at) => {
                self.selection.remove(at);
            }
            Err(at) => self.selection.insert(at, index),
        }
        self.follows = false;
    }
    fn selection_changed(&mut self) {
        self.announced += 1;
    }
    fn restore_order(&mut self, order: Order) {
        self.sorted.push(format!("{}:{}", order.key, order.desc));
        self.order = order;
        self.selection.clear();
        self.thumbs_stale = false;
        self.sizes_stale = false;
        self.cursor = 0;
        self.windows.push(format!("0:{}", self.window_size));
    }
    fn sort(&mut self, order: Order) {
        self.sorted.push(format!("{}:{}", order.key, order.desc));
        self.order = order;
        self.windows.push(format!("0:{}", self.window_size));
    }
    fn window(&mut self, start: usize, count: usize) {
        self.windows.push(format!("{start}:{count}"));
    }
    fn can_locate(&self) -> bool {
        self.can_locate
    }
    fn locate(&mut self, path: &str) {
        self.located.push(path.into());
    }
    fn peek(&mut self, path: &str, first: usize, hidden: bool, hidden_last: bool) {
        self.peeks.push((path.into(), first, hidden, hidden_last));
    }
    fn say(&mut self, say: Say) -> u64 {
        let text = say.text(&self.catalog);
        self.message(&text, false);
        self.notice_serial
    }
    fn withdraw(&mut self, serial: u64) {
        if serial == self.notice_serial {
            self.notice.clear();
        }
    }
}

// One pane and its strip.
pub struct Side {
    pub state: TabState,
    pub pane: Fake,
}

impl Side {
    pub fn new(pane: Fake) -> Self {
        Self {
            state: TabState::default(),
            pane,
        }
    }

    // The shipped bar fixture's pair: a pane on one folder and a second tab opened on it.
    pub fn pair(path: &str) -> Self {
        let mut side = Self::new(Fake::at(path));
        side.state.open_new(&mut side.pane, None);
        side
    }

    pub fn count(&self) -> usize {
        self.state.count()
    }

    pub fn act(&mut self, action: &str) -> bool {
        self.state.act(action, &mut self.pane)
    }

    pub fn paths(&self) -> Vec<String> {
        self.state
            .tabs
            .as_ref()
            .map(|tabs| tabs.items.iter().map(|tab| tab.path.clone()).collect())
            .unwrap_or_default()
    }

    pub fn identities(&self) -> Vec<u64> {
        self.state
            .tabs
            .as_ref()
            .map(|tabs| tabs.items.iter().map(|tab| tab.identity).collect())
            .unwrap_or_default()
    }
}

// The window's panes as the strip reaches them; a None side is a pane that is gone.
pub struct World {
    pub sides: Vec<Option<Side>>,
}

impl World {
    pub fn one(side: Side) -> Self {
        Self {
            sides: vec![Some(side)],
        }
    }

    pub fn side(&mut self, at: usize) -> &mut Side {
        self.sides[at].as_mut().unwrap()
    }
}

impl Panes for World {
    fn visit(&mut self, side: usize, f: &mut dyn FnMut(&mut TabState, &mut dyn Live)) -> bool {
        match self.sides.get_mut(side).and_then(Option::as_mut) {
            Some(side) => {
                f(&mut side.state, &mut side.pane);
                true
            }
            None => false,
        }
    }
}

// The shipped bar fixture's strip: pid 111, 500 wide, standing for pane 0.
pub fn bar() -> Bar {
    let mut bar = Bar::new("111");
    bar.width = 500.;
    bar.pane = Some(0);
    bar
}

pub fn acks(bar: &mut Bar) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut next = bar.pump_taken();
    while let Some((_, token)) = next {
        tokens.push(token);
        next = bar.taken_exited();
    }
    tokens
}

#[path = "g1_10/counters.rs"]
mod counters;
#[path = "g1_10/ipc.rs"]
mod ipc;
#[path = "g1_10/moves.rs"]
mod moves;
#[path = "g1_10/receive.rs"]
mod receive;
#[path = "g1_10/restore.rs"]
mod restore;
#[path = "g1_10/strings.rs"]
mod strings;
#[path = "g1_10/strip.rs"]
mod strip;
#[path = "g1_10/switch.rs"]
mod switch;
#[path = "g1_10/transfer.rs"]
mod transfer;
