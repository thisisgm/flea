//! Where a pane has been and how it gets back; it submits typed requests to the pane's swap and transport.
pub mod anchor;
pub mod folder_state;
pub mod history;
pub mod sort;
pub mod watch;
mod landing;
mod steps;

use crate::pane::{requests::RowAction, swap::Options, Pane};
use anchor::{Anchor, Preference};
use folder_state::{Folders, Order, Writer};
use history::{History, Restore};
use std::{cell::RefCell, rc::Rc};
use watch::{Gate, Watch};

// Locate ids for the anchor sit above the signed menu ids, keeping the reply askers disjoint.
pub const LOCATE_ID_FLOOR: u64 = 2_147_483_648;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listing {
    pub path: String,
    pub first: usize,
    pub hidden: bool,
    pub want_changed: bool,
    pub order: Order,
    pub folders_first: bool,
    pub group_by_kind: bool,
    pub hidden_last: bool,
    pub select: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    List(Listing),
    Window {
        start: usize,
        count: usize,
    },
    Sort {
        order: Order,
        folders_first: bool,
        group_by_kind: bool,
        hidden_last: bool,
    },
    Locate {
        path: String,
    },
    LocateMany {
        paths: Vec<String>,
        id: u64,
    },
    Paths(RowAction),
    Persist(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    Busy,
    NotLoaded,
}

impl Refusal {
    pub fn key(&self) -> &'static str {
        match self {
            Self::Busy => "pane.busy",
            Self::NotLoaded => "navigation.row_not_loaded",
        }
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counters {
    pub lists: usize,
    pub windows: usize,
    pub sorts: usize,
    pub locates: usize,
    pub persisted: usize,
}

#[derive(Default)]
pub struct Navigator {
    pub history: History,
    pub restore: Option<Restore>,
    pub anchor: Option<Anchor>,
    pub preference: Option<Preference>,
    pub watch: Watch,
    pub gate: Gate,
    pub order: Order,
    pub preserve_sort: bool,
    pub pending_sort: Option<sort::Request>,
    pub pending_select: Option<String>,
    pub folders: Rc<RefCell<Folders>>,
    pub writer: Rc<RefCell<Writer>>,
    pub counters: Counters,
    pub out: Vec<Command>,
    seen: usize,
    locate_seq: u64,
}

impl Navigator {
    pub fn new(folders: Rc<RefCell<Folders>>, writer: Rc<RefCell<Writer>>) -> Self {
        Self {
            folders,
            writer,
            locate_seq: LOCATE_ID_FLOOR,
            ..Self::default()
        }
    }

    pub fn take(&mut self) -> Vec<Command> {
        std::mem::take(&mut self.out)
    }

    fn emit(&mut self, command: Command) {
        match &command {
            Command::List(_) => self.counters.lists += 1,
            Command::Window { .. } => self.counters.windows += 1,
            Command::Sort { .. } => self.counters.sorts += 1,
            Command::Locate { .. } | Command::LocateMany { .. } => self.counters.locates += 1,
            Command::Persist(_) => self.counters.persisted += 1,
            Command::Paths(_) => {}
        }
        self.out.push(command);
    }

    pub fn next_locate(&mut self) -> u64 {
        self.locate_seq += 1;
        self.locate_seq
    }

    // A re-read renumbers every row, so it waits while an interaction or an unfinished anchor owns them.
    pub fn busy(&self, pane: &Pane) -> bool {
        pane.busy()
            || self.gate.owned()
            || self
                .anchor
                .as_ref()
                .is_some_and(|anchor| anchor.path == pane.path())
    }

    // The order the listing is actually in, which the header's mark draws.
    pub fn sort_mark(&self) -> String {
        sort::mark(self.order)
    }

    // The listing every navigation and re-read submits, in the folder's own order.
    pub fn listing(&mut self, pane: &Pane, path: &str, want_changed: bool) -> Listing {
        let folders = self.folders.borrow();
        if !self.preserve_sort {
            self.order = folders.order_for(path);
        }
        Listing {
            path: path.into(),
            first: pane.first_count(),
            hidden: folders.hidden,
            want_changed,
            order: self.order,
            folders_first: folders.folders_first,
            group_by_kind: folders.group_by_kind,
            hidden_last: folders.hidden_last,
            select: self
                .pending_select
                .clone()
                .filter(|select| !select.is_empty())
                .or_else(|| {
                    self.anchor
                        .as_ref()
                        .filter(|anchor| anchor.path == path)
                        .and_then(Anchor::target)
                }),
        }
    }

    // The single funnel: tab switches, Back, Up, rail clicks and re-reads all land here.
    pub fn open_without_history(
        &mut self,
        pane: &mut Pane,
        path: &str,
        mut options: Options,
        restore: Option<Restore>,
        want_changed: bool,
    ) -> Result<bool, Refusal> {
        if pane.busy() {
            if !options.supersede {
                return Err(Refusal::Busy);
            }
            self.restore = None;
        }
        options.restore_name = restore
            .as_ref()
            .map(|restore| restore.entry.name.clone())
            .filter(|name| !name.is_empty());
        let now = pane.now();
        if !pane.begin(path.into(), options, now) {
            return Ok(false);
        }
        self.restore = restore;
        self.pending_sort = None;
        let listing = self.listing(pane, path, want_changed);
        self.emit(Command::List(listing));
        Ok(true)
    }
}
