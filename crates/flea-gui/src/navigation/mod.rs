//! Where a pane has been and how it gets back; it submits typed requests to the pane's swap and transport.
pub mod anchor;
pub mod folder_state;
mod fresh;
pub mod history;
mod landing;
mod orders;
pub(crate) mod position;
mod retry;
mod selected;
pub mod sort;
mod steps;
pub mod watch;

use crate::columns::menu::Deferred;
use crate::pane::{requests::RowAction, swap::Options, Pane};
use anchor::{Anchor, Preference};
use folder_state::{Folders, Order, Writer};
use history::{History, Restore};
use orders::{Orders, RequestedOrder};
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
    ListPaths(Listing, Vec<String>),
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
    /// ui.json "keepScreenLine" for this pane's window, set by the shell before each frame.
    pub(crate) keep_screen_line: bool,
    pub(crate) lines: position::Lines,
    pub history: History,
    pub restore: Option<Restore>,
    pub anchor: Option<Anchor>,
    pub(crate) deletion_anchor: Option<Anchor>,
    pub(crate) viewport: Option<anchor::Viewport>,
    pub(crate) grid_landing: Option<(usize, usize, f32)>,
    pub preference: Option<Preference>,
    pub watch: Watch,
    pub gate: Gate,
    pub order: Order,
    pub committed_sort: Option<Order>,
    pub preserve_sort: bool,
    pub pending_sort: Option<sort::Request>,
    pub pending_select: Option<String>,
    pub(crate) refresh_owed: Option<(String, Option<String>)>,
    pub(crate) select_locate: Option<(usize, u64)>,
    /// The listing and row a navigation selected on landing, which Escape climbs through as no mark.
    pub landed: Option<(usize, usize)>,
    /// The row a re-read marked again from a lone mark, owed a lone take by the selection.
    pub(crate) lone_mark: Option<usize>,
    /// A manual reload out: its folder and the count it left, then the changed count the backend named.
    pub reload_from: Option<(String, usize)>,
    pub reload_changed: Option<usize>,
    /// The rows a landed reload changed, owed to the status line as its notice.
    pub reloaded: Option<usize>,
    pub retry: Option<retry::Retry>,
    pub menus: Deferred,
    pub background_at: Option<(f32, f32)>,
    pub folders: Rc<RefCell<Folders>>,
    pub writer: Rc<RefCell<Writer>>,
    pub counters: Counters,
    pub out: Vec<Command>,
    orders: Orders,
    seen: usize,
    locate_seq: u64,
    relisting: bool,
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
            Command::List(_) | Command::ListPaths(..) => self.counters.lists += 1,
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
            || self.select_locate.is_some()
            || self
                .anchor
                .as_ref()
                .is_some_and(|anchor| anchor.path == pane.path())
    }

    // The latest requested order, retained for the existing command and IPC contract.
    pub fn sort_mark(&self) -> String {
        sort::mark(self.order)
    }

    // The header follows the retained row generation, never a command still waiting for rows.
    pub fn drawn_order(&self) -> Order {
        self.orders.drawn.map_or(self.order, |(_, order)| order)
    }

    fn sync_orders(&mut self, pane: &Pane) {
        let landed = pane.held.listing != 0
            && self
                .orders
                .drawn
                .is_none_or(|(listing, _)| listing != pane.held.listing);
        if landed {
            let order = self
                .orders
                .requests
                .pop_front()
                .map(|request| {
                    if request.sort {
                        self.committed_sort = Some(request.order);
                    }
                    request.order
                })
                .unwrap_or(self.order);
            self.orders.drawn = Some((pane.held.listing, order));
        }
        if !pane.busy()
            && matches!(
                pane.state.kind,
                crate::pane::Kind::Error | crate::pane::Kind::Locked
            )
        {
            self.orders.requests.clear();
        }
    }

    fn start_sort(&mut self, pane: &Pane, order: Order) {
        self.sync_orders(pane);
        self.orders.path = pane.path().into();
        self.orders.explicit = true;
        let folders = self.folders.borrow();
        self.orders.preferences = Some((folders.global, folders.remember_sort));
        self.orders
            .requests
            .push_back(RequestedOrder { order, sort: true });
    }

    // The listing every navigation and re-read submits, in the folder's own order.
    pub fn listing(&mut self, pane: &Pane, path: &str, want_changed: bool) -> Listing {
        let folders = self.folders.borrow();
        let preferences = (folders.global, folders.remember_sort);
        if !self.preserve_sort
            && (self.orders.path != path
                || !self.orders.explicit && self.orders.preferences != Some(preferences))
        {
            self.order = folders.order_for(path);
        }
        if self.orders.path != path {
            self.orders.explicit = false;
            self.orders.requests.clear();
        } else {
            // A superseded scan may be cancelled, while earlier sorts still answer in command order.
            self.orders.requests.retain(|request| request.sort);
        }
        self.orders.path = path.into();
        self.orders.preferences = Some(preferences);
        self.orders.requests.push_back(RequestedOrder {
            order: self.order,
            sort: false,
        });
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
        if self.keep_screen_line {
            self.lines.remember(pane);
        }
        if pane.busy() {
            if !options.supersede {
                return Err(Refusal::Busy);
            }
            self.restore = None;
        }
        // A hop elsewhere drops a waiting background menu, while the armed hop keeps its own request.
        self.menus.hop(path);
        options.restore_name = restore
            .as_ref()
            .map(|restore| restore.entry.name.clone())
            .filter(|name| !name.is_empty())
            .or(options.restore_name);
        let relisting = options.relist;
        self.sync_orders(pane);
        if pane.swap.unheld_total.is_some() {
            let sorts = self
                .orders
                .requests
                .iter()
                .filter(|request| request.sort)
                .count();
            let mut listed = sorts.saturating_sub(pane.swap.sorts_out.len());
            // Begin drops an already listed pair, so its remaining window cannot commit this navigation.
            self.orders.requests.retain(|request| {
                if request.sort && listed > 0 {
                    listed -= 1;
                    false
                } else {
                    true
                }
            });
        }
        let now = pane.now();
        if !pane.begin(path.into(), options, now) {
            return Ok(false);
        }
        self.refresh_owed = None;
        self.select_locate = None;
        self.relisting = relisting;
        self.restore = restore;
        self.pending_sort = None;
        let listing = self.listing(pane, path, want_changed);
        self.emit(Command::List(listing));
        Ok(true)
    }
}

#[cfg(test)]
mod order_tests {
    use super::*;
    use flea::protocol::{Reply, RowFacts};

    fn pair(nav: &mut Navigator, pane: &mut Pane, listing: usize) {
        let before = pane.path().to_owned();
        for reply in [
            Reply::Listed {
                path: "/A".into(),
                total: 1,
            },
            Reply::Rows {
                start: 0,
                rows: vec![RowFacts::default()],
                listing,
            },
        ] {
            pane.receive(reply).unwrap();
            nav.after_reply(pane, &before, &mut Vec::new());
        }
    }

    #[test]
    fn m9_g1rows_05_forget_joins_the_old_sort_and_default_list_to_their_own_rows() {
        for listed in [false, true] {
            let data = flea::uistate::from_file("{}");
            let mut nav = Navigator::new(
                Rc::new(RefCell::new(Folders::from_settings(&data, Vec::new()))),
                Rc::default(),
            );
            let mut pane = Pane::new("/A".into(), 620.);
            nav.listing(&pane, "/A", false);
            pair(&mut nav, &mut pane, 1);
            let size = Order {
                key: "size",
                desc: true,
            };
            assert_eq!(
                nav.resort(&mut pane, size, &mut Vec::new(), String::new()),
                Ok(true)
            );
            nav.take();
            if listed {
                pane.receive(Reply::Listed {
                    path: "/A".into(),
                    total: 1,
                })
                .unwrap();
                nav.after_reply(&mut pane, "/A", &mut Vec::new());
            }
            assert_eq!(nav.forget_sort(&mut pane, String::new()), Ok(true));
            if listed {
                assert!(!pane
                    .receive(Reply::Rows {
                        start: 0,
                        rows: vec![RowFacts::default()],
                        listing: 2
                    })
                    .unwrap());
                nav.after_reply(&mut pane, "/A", &mut Vec::new());
                assert_eq!(nav.drawn_order(), Order::default());
            } else {
                pair(&mut nav, &mut pane, 2);
                assert_eq!(
                    nav.drawn_order(),
                    size,
                    "the earlier sort's rows arrive before the default list"
                );
            }
            assert_eq!(nav.sort_snapshot(&pane), Err(Refusal::Busy));
            pair(&mut nav, &mut pane, 3);
            assert_eq!(nav.drawn_order(), Order::default());
            assert_eq!(nav.order, Order::default());
            assert_eq!(nav.sort_snapshot(&pane).unwrap().order, Order::default());
        }
    }
}
