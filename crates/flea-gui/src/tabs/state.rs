//! The strip's tab list: hidden tabs are snapshots, so the pane and its backend still own one listing.
use crate::navigation::{folder_state::Order, history::Entry};
use flea::{i18n::I18n, jsondoc::Json};
#[path = "restore.rs"]
mod restore;
pub use restore::restore_plan;

// The nine-tab cap matches the TUI's direct digit selection; the GUI's shortcuts cycle the same state.
pub const MAX: usize = 9;

// What a held row tells the strip: its listed name and whether it is a folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub name: String,
    pub dir: bool,
}

// How a tab asks for its folder; order is the tab's own, so the listing can arrive already sorted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Open {
    pub keep_hidden: bool,
    pub clear_at_once: bool,
    pub supersede: bool,
    pub order: Option<Order>,
}

// Every verdict the strip says, named by its catalog key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Say {
    NoTab(usize),
    LastTab,
    Most,
    OnlyFolder,
    NotReceived,
    FolderGone,
    FinishRename,
    Busy,
}

impl Say {
    pub fn key(&self) -> &'static str {
        match self {
            Self::NoTab(_) => "tabs.no_tab",
            Self::LastTab => "tabs.last_tab",
            Self::Most => "tabs.most",
            Self::OnlyFolder => "tabs.only_folder",
            Self::NotReceived => "tabs.not_received",
            Self::FolderGone => "tabs.folder_gone",
            Self::FinishRename => "tabs.finish_rename",
            Self::Busy => "pane.busy",
        }
    }

    pub fn text(&self, catalog: &I18n) -> String {
        match self {
            Self::NoTab(number) => catalog.tr_args(self.key(), &[&number.to_string()]),
            _ => catalog.tr(self.key()),
        }
    }
}

// The pane a tab operation reads and drives; the window's pane host and the tests' fake both answer it.
pub trait Live {
    fn path(&self) -> String;
    // Where a snapshotted tab reopens: the folder a search or a history stands over, else the place.
    fn resting_path(&self) -> String;
    fn search_live(&self) -> bool;
    // The folder a search row's relative name joins, the pane's path when no search is open.
    fn search_base(&self) -> String;
    fn busy(&self) -> bool;
    fn drop_path(&self) -> String;
    fn home(&self) -> String;
    fn history(&self) -> (Vec<Entry>, Vec<Entry>);
    fn set_history(&mut self, back: Vec<Entry>, forward: Vec<Entry>);
    fn cursor(&self) -> usize;
    fn row(&self, index: usize) -> Option<Row>;
    // The first held row and how many rows the held window spans.
    fn held(&self) -> (usize, usize);
    fn total(&self) -> usize;
    fn window_size(&self) -> usize;
    fn view(&self) -> &'static str;
    // A tab's view becomes the standing one unless its folder keeps its own.
    fn adopt_view(&mut self, path: &str, view: &'static str);
    fn hidden(&self) -> bool;
    fn set_hidden(&mut self, hidden: bool);
    fn selected(&self) -> Vec<usize>;
    fn follows(&self) -> bool;
    fn order(&self) -> Order;
    fn sort_snapshot(&self) -> Option<crate::navigation::sort::Snapshot> {
        (!self.busy() && !self.search_live()).then(|| crate::navigation::sort::Snapshot {
            path: self.resting_path(),
            order: self.order(),
        })
    }
    fn list_requests(&self) -> usize;
    fn dev(&self) -> u64;
    fn rename_pending(&self) -> bool;
    fn new_tab_path(&self, here: &str) -> String;
    fn close_preview(&mut self);
    // Drops a search, a history or a trash view over the listing; true when the rows must be listed again.
    fn drop_overlay(&mut self) -> bool;
    fn open(&mut self, path: &str, open: Open);
    fn set_cursor(&mut self, index: usize);
    fn clear_selection(&mut self);
    fn select_only(&mut self, index: usize);
    fn toggle(&mut self, index: usize);
    fn selection_changed(&mut self);
    // The reset a tab's own order takes (cleared selection, cursor on top, first window), remembering nothing for the folder.
    fn restore_order(&mut self, order: Order);
    // A bare sort and its first window, which remembers nothing for the folder.
    fn sort(&mut self, order: Order);
    fn window(&mut self, start: usize, count: usize);
    fn can_locate(&self) -> bool;
    fn locate(&mut self, path: &str);
    // A folder check through the backend's peek, answered by a peeked line echoing these four.
    fn peek(&mut self, path: &str, first: usize, hidden: bool, hidden_last: bool);
    // Shows a verdict and answers the serial of the notice it stands as.
    fn say(&mut self, say: Say) -> u64;
    fn withdraw(&mut self, serial: u64);
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tab {
    pub identity: u64,
    pub path: String,
    pub back: Vec<Entry>,
    pub forward: Vec<Entry>,
    pub cursor: usize,
    pub cursor_name: String,
    pub view: &'static str,
    pub hidden: bool,
    pub selected: Vec<usize>,
    // A lone row restores as the only one, so a switch never re-arms the row a tap left behind.
    pub follows: bool,
    pub order: Order,
    // The counter every listing bumps, so a selection only returns to its own rows.
    pub list_requests: usize,
    // The folder's filesystem, so a drop on this tab while another shows decides move against copy.
    pub dev: u64,
}

impl Tab {
    // A tab known only by its folder, the shape a saved strip names before its first visit.
    pub fn at(path: impl Into<String>) -> Self {
        Self {
            identity: 0,
            path: path.into(),
            back: Vec::new(),
            forward: Vec::new(),
            cursor: 0,
            cursor_name: String::new(),
            view: "list",
            hidden: false,
            selected: Vec::new(),
            follows: false,
            order: Order::default(),
            list_requests: 0,
            dev: 0,
        }
    }
}

// What the current tab's listing still owes once its rows land.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pending {
    pub cursor: Option<usize>,
    pub cursor_name: Option<String>,
    pub locate_path: String,
    pub order: Option<Order>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tabs {
    pub items: Vec<Tab>,
    pub index: usize,
    pub pending: Pending,
}

// keep is the landing the current tab still owes, for a change that leaves the current tab's listing out.
pub fn pack(items: Vec<Tab>, index: usize, keep: Option<&Pending>) -> Tabs {
    Tabs {
        items,
        index,
        pending: keep.cloned().unwrap_or_default(),
    }
}

// The saved schema's lastTabs value: the strip's folders in order and the current tab.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub paths: Vec<String>,
    pub index: usize,
}

impl Plan {
    pub fn to_json(&self) -> String {
        let paths: Vec<String> = self
            .paths
            .iter()
            .map(|path| format!("\"{}\"", flea::json::escape(path)))
            .collect();
        format!(
            "{{\"paths\":[{}],\"index\":{}}}",
            paths.join(","),
            self.index
        )
    }
}

// The values lastPath and lastTabs hold, so a navigation writes only the keys it changed and a repeat writes nothing.
#[derive(Debug, Default)]
pub struct Remember {
    path: Option<String>,
    tabs: Option<String>,
    pub writes: usize,
}

impl Remember {
    pub fn seeded(state: &Json) -> Self {
        let tabs = state.get("lastTabs").and_then(|stored| {
            let paths = stored.get("paths")?.as_array()?;
            let paths = paths
                .iter()
                .map(|path| path.as_str().map(str::to_owned))
                .collect::<Option<Vec<_>>>()?;
            let index = stored
                .get("index")?
                .as_f64()
                .filter(|index| index.fract() == 0. && *index >= 0.)?;
            Some(
                Plan {
                    paths,
                    index: index as usize,
                }
                .to_json(),
            )
        });
        Self {
            path: state
                .get("lastPath")
                .and_then(Json::as_str)
                .map(str::to_owned),
            tabs,
            writes: 0,
        }
    }

    // The patch a strip or path change owes; a Dual window, a listing out or no folder yet writes nothing.
    pub fn owed(&mut self, path: &str, plan: &Plan, dual: bool, busy: bool) -> Option<String> {
        if dual || busy || path.is_empty() {
            return None;
        }
        let tabs = plan.to_json();
        let mut keys = Vec::new();
        if self.path.as_deref() != Some(path) {
            keys.push(format!("\"lastPath\":\"{}\"", flea::json::escape(path)));
        }
        if self.tabs.as_deref() != Some(tabs.as_str()) {
            keys.push(format!("\"lastTabs\":{tabs}"));
        }
        if keys.is_empty() {
            return None;
        }
        self.path = Some(path.into());
        self.tabs = Some(tabs);
        self.writes += 1;
        Some(format!("{{{}}}", keys.join(",")))
    }
}

// One pane's strip; None until a second tab exists, so a lone tab costs nothing.
#[derive(Debug, Default)]
pub struct TabState {
    pub tabs: Option<Tabs>,
    next_identity: u64,
    // The serial of a standing count verdict, withdrawn when the count changes.
    pub count_notice: Option<u64>,
}

// "/" and Home draw their own words; any other folder draws its leaf, read off its tilde form.
pub fn label(path: &str, home: &str, home_label: &str) -> String {
    if path.is_empty() || path == "/" {
        return "/".into();
    }
    if !home.is_empty() && path == home {
        return home_label.into();
    }
    let display = if !home.is_empty() && path.starts_with(&format!("{home}/")) {
        format!("~{}", &path[home.len()..])
    } else {
        path.to_owned()
    };
    let leaf = match display.rfind('/') {
        None => display.as_str(),
        Some(cut) if cut == display.len() - 1 => display.as_str(),
        Some(cut) => &display[cut + 1..],
    };
    if leaf.is_empty() {
        path.into()
    } else {
        leaf.into()
    }
}

// Only the held cursor row can name a file; a search's row never belongs to its resting folder.
pub fn cursor_file(pane: &dyn Live) -> String {
    pane.row(pane.cursor())
        .map(|row| row.name)
        .unwrap_or_default()
}

fn join(folder: &str, name: &str) -> String {
    if folder == "/" {
        format!("/{name}")
    } else {
        format!("{folder}/{name}")
    }
}

// Only ever called for a switch that re-listed nothing; the clamp is the belt on top of that.
pub fn restore_selection(pane: &mut dyn Live, selected: &[usize], follows: bool) {
    pane.clear_selection();
    if selected.is_empty() {
        return;
    }
    let total = pane.total();
    if follows && selected.len() == 1 && selected[0] < total {
        pane.select_only(selected[0]);
        pane.selection_changed();
        return;
    }
    let mut kept = 0;
    for &row in selected {
        if row < total {
            pane.toggle(row);
            kept += 1;
        }
    }
    if kept > 0 {
        pane.selection_changed();
    }
}

impl TabState {
    pub fn count(&self) -> usize {
        self.tabs.as_ref().map_or(1, |tabs| tabs.items.len().max(1))
    }

    pub fn current(&self) -> usize {
        self.tabs.as_ref().map_or(0, |tabs| tabs.index)
    }

    pub fn identity(&mut self) -> u64 {
        self.next_identity += 1;
        self.next_identity
    }

    // The path tab i draws: the live path for the current tab, the snapshot's for a hidden one.
    pub fn path_at(&self, i: usize, current_path: &str) -> String {
        if i == self.current() {
            return current_path.into();
        }
        self.tabs
            .as_ref()
            .and_then(|tabs| tabs.items.get(i))
            .map(|tab| tab.path.clone())
            .unwrap_or_default()
    }

    // The tab's filesystem the same way, 0 when unknown, which a drop reads as copy.
    pub fn dev_at(&self, i: usize, current_dev: u64) -> u64 {
        if i == self.current() {
            return current_dev;
        }
        self.tabs
            .as_ref()
            .and_then(|tabs| tabs.items.get(i))
            .map_or(0, |tab| tab.dev)
    }

    pub fn labels(&self, current_path: &str, home: &str, home_label: &str) -> Vec<String> {
        (0..self.count())
            .map(|i| label(&self.path_at(i, current_path), home, home_label))
            .collect()
    }

    // The mark names a standing notice's serial, never its sentence; a count change withdraws it.
    fn say_count(&mut self, pane: &mut dyn Live, say: Say) {
        self.count_notice = Some(pane.say(say));
    }

    // Any other verdict replaces a standing count one, so no later change withdraws it.
    fn say_plain(&mut self, pane: &mut dyn Live, say: Say) {
        self.count_notice = None;
        pane.say(say);
    }

    fn drop_count(&mut self, pane: &mut dyn Live) {
        if let Some(serial) = self.count_notice.take() {
            pane.withdraw(serial);
        }
    }

    pub fn snapshot(
        &mut self,
        pane: &dyn Live,
        path: Option<String>,
        identity: u64,
        dropped: bool,
    ) -> Tab {
        let at = path.unwrap_or_else(|| pane.resting_path());
        // A cursor and a selection read off a search's own listing name nothing in the folder the tab records.
        let elsewhere = at != pane.path() || pane.search_live() || dropped;
        let (back, forward) = pane.history();
        Tab {
            identity: if identity != 0 {
                identity
            } else {
                self.identity()
            },
            path: at,
            back,
            forward,
            cursor: if elsewhere { 0 } else { pane.cursor() },
            cursor_name: if elsewhere {
                String::new()
            } else {
                cursor_file(pane)
            },
            view: pane.view(),
            hidden: pane.hidden(),
            selected: if elsewhere {
                Vec::new()
            } else {
                pane.selected()
            },
            follows: !elsewhere && pane.follows(),
            order: pane.order(),
            list_requests: pane.list_requests(),
            dev: if elsewhere { 0 } else { pane.dev() },
        }
    }

    pub fn current_items(&mut self, pane: &dyn Live, here: Option<String>) -> Vec<Tab> {
        match &self.tabs {
            Some(tabs) if !tabs.items.is_empty() => tabs.items.clone(),
            _ => vec![self.snapshot(pane, here, 0, false)],
        }
    }

    fn pending_mut(&mut self) -> Option<&mut Pending> {
        self.tabs.as_mut().map(|tabs| &mut tabs.pending)
    }

    // Records the tab being left, keeping its identity.
    fn leave(&mut self, pane: &dyn Live, items: &mut [Tab], here: &str, dropped: bool) {
        let index = self.current().min(items.len().saturating_sub(1));
        let identity = items[index].identity;
        items[index] = self.snapshot(pane, Some(here.into()), identity, dropped);
    }

    pub fn apply(
        &mut self,
        pane: &mut dyn Live,
        item: &Tab,
        dropped: bool,
        cursor: Option<String>,
    ) {
        // Rows from a search dropped onto its scope, or a listing still out, are not the rows this tab left.
        let same = pane.path() == item.path
            && pane.hidden() == item.hidden
            && !dropped
            && cursor.is_none()
            && !pane.busy();
        if let Some(pending) = self.pending_mut() {
            pending.cursor_name = cursor.clone();
        }
        let shown = pane.view();
        pane.set_history(item.back.clone(), item.forward.clone());
        pane.adopt_view(&item.path, item.view);
        pane.set_hidden(item.hidden);
        if same {
            if pane.order() != item.order {
                pane.restore_order(item.order);
                if let Some(pending) = self.pending_mut() {
                    pending.cursor = Some(item.cursor);
                }
                return;
            }
            // The switch that re-reads nothing, unless something else did: the watch and a write both list.
            pane.set_cursor(item.cursor);
            if item.list_requests == pane.list_requests() {
                restore_selection(pane, &item.selected, item.follows);
                return;
            }
            pane.clear_selection();
            return;
        }
        // The listing keeps the tab's own dotfile answer; a tab in another view clears at once.
        let clear_at_once = pane.view() != shown;
        pane.open(
            &item.path,
            Open {
                keep_hidden: true,
                clear_at_once,
                supersede: true,
                order: Some(item.order),
            },
        );
        // After the request: this landing is owed to the listing just asked for.
        if let Some(pending) = self.pending_mut() {
            pending.cursor = Some(if cursor.is_none() { item.cursor } else { 0 });
            pending.cursor_name = cursor;
            pending.order = Some(item.order);
        }
    }

    pub fn apply_pending(&mut self, pane: &mut dyn Live) {
        let Some(tabs) = self.tabs.as_mut() else {
            return;
        };
        let pending = &mut tabs.pending;
        // Spent on the first reply either way, or an order already in force never was.
        if let Some(order) = pending.order.take() {
            if pane.order() != order {
                pane.sort(order);
                return;
            }
        }
        if let Some(name) = pending.cursor_name.take().filter(|name| !name.is_empty()) {
            let (first, held) = pane.held();
            let end = pane.total().min(first + held);
            let found = (first..end).find(|&i| pane.row(i).is_some_and(|row| row.name == name));
            match found {
                Some(i) => pending.cursor = Some(i),
                None if pane.total() > 0 && pane.can_locate() => {
                    pending.locate_path = join(&pane.path(), &name);
                    pending.cursor = None;
                    pane.locate(&pending.locate_path.clone());
                    return;
                }
                None => {}
            }
        }
        if let Some(cursor) = pending.cursor.take() {
            let last = pane.total().saturating_sub(1);
            pane.set_cursor(cursor.min(last));
        }
    }

    // A receiver's index comes from its own name lookup, never from the source window.
    pub fn located(
        &mut self,
        pane: &mut dyn Live,
        path: &str,
        directory: &str,
        index: Option<usize>,
    ) {
        let Some(tabs) = self.tabs.as_mut() else {
            return;
        };
        let pending = &mut tabs.pending;
        if pending.locate_path.is_empty() || path != pending.locate_path || directory != pane.path()
        {
            return;
        }
        pending.locate_path.clear();
        if pane.busy() {
            return;
        }
        let total = pane.total();
        let index = index.filter(|&index| index < total).unwrap_or(0);
        pane.set_cursor(index);
        if total > 0 && pane.row(index).is_none() {
            let count = pane.window_size();
            pane.window(index, count);
        }
    }

    // A launch carries a filename to land on, as a torn-off tab does.
    pub fn prepare_cursor(&mut self, pane: &dyn Live, cursor: &str) {
        if cursor.is_empty() {
            return;
        }
        if self.tabs.is_none() {
            let first = self.snapshot(pane, None, 0, false);
            self.tabs = Some(pack(vec![first], 0, None));
        }
        if let Some(pending) = self.pending_mut() {
            pending.cursor = Some(0);
            pending.cursor_name = Some(cursor.into());
            pending.locate_path.clear();
        }
    }

    // A caller may name where the new tab lands; without one the Opening setting decides.
    pub fn open_new(&mut self, pane: &mut dyn Live, place: Option<String>) {
        // Before the preview and the search go: a refused tenth tab must cost the user nothing.
        if self.count() >= MAX {
            self.say_count(pane, Say::Most);
            return;
        }
        let here = pane.resting_path();
        pane.close_preview();
        let dropped = pane.drop_overlay();
        let target = place
            .filter(|place| !place.is_empty())
            .unwrap_or_else(|| pane.new_tab_path(&here));
        let mut items = self.current_items(pane, Some(here.clone()));
        self.leave(pane, &mut items, &here, dropped);
        let fresh = self.snapshot(pane, Some(target.clone()), 0, dropped);
        items.push(fresh);
        let index = items.len() - 1;
        self.tabs = Some(pack(items, index, None));
        self.drop_count(pane);
        // A dropped search still stands on its rows and a listing out belongs to the tab left, so both list again.
        if pane.path() != target || dropped || pane.busy() {
            pane.open(
                &target,
                Open {
                    supersede: true,
                    ..Open::default()
                },
            );
        }
    }

    // Ctrl+Return on the cursor row, the keyboard twin of a middle click: that folder in a new tab.
    pub fn open_cursor_tab(&mut self, pane: &mut dyn Live) {
        let row = pane.row(pane.cursor());
        let Some(row) = row.filter(|row| row.dir) else {
            self.say_plain(pane, Say::OnlyFolder);
            return;
        };
        let target = join(&pane.search_base(), &row.name);
        self.open_new(pane, Some(target));
    }

    pub fn select_at(&mut self, pane: &mut dyn Live, i: usize) {
        let here = pane.resting_path();
        let mut items = self.current_items(pane, Some(here.clone()));
        if i >= items.len() {
            self.say_count(pane, Say::NoTab(i + 1));
            return;
        }
        if i == self.current() {
            return;
        }
        pane.close_preview();
        let dropped = pane.drop_overlay();
        self.leave(pane, &mut items, &here, dropped);
        let item = items[i].clone();
        self.tabs = Some(pack(items, i, None));
        self.apply(pane, &item, dropped, None);
    }

    pub fn close_at(&mut self, pane: &mut dyn Live, i: usize) {
        let mut items = self.current_items(pane, None);
        if items.len() <= 1 {
            self.say_count(pane, Say::LastTab);
            return;
        }
        if i >= items.len() {
            self.say_count(pane, Say::NoTab(i + 1));
            return;
        }
        let index = self.current();
        items.remove(i);
        let next = match i.cmp(&index) {
            std::cmp::Ordering::Less => index - 1,
            std::cmp::Ordering::Equal => i.min(items.len() - 1),
            std::cmp::Ordering::Greater => index,
        };
        if i == index {
            pane.close_preview();
            let dropped = pane.drop_overlay();
            let item = items[next].clone();
            self.tabs = Some(pack(items, next, None));
            self.apply(pane, &item, dropped, None);
        } else {
            let keep = self.tabs.as_ref().map(|tabs| tabs.pending.clone());
            self.tabs = Some(pack(items, next, keep.as_ref()));
        }
        self.drop_count(pane);
    }

    // Every tab but i closes; the current tab's listing is owed only when i was hidden.
    pub fn close_others(&mut self, pane: &mut dyn Live, i: usize) {
        let count = self.count();
        if count <= 1 {
            self.say_count(pane, Say::LastTab);
            return;
        }
        if i >= count {
            self.say_count(pane, Say::NoTab(i + 1));
            return;
        }
        if i != self.current() {
            self.select_at(pane, i);
        }
        if let Some(tabs) = self.tabs.as_mut() {
            let kept = tabs.items.swap_remove(tabs.index);
            tabs.items = vec![kept];
            tabs.index = 0;
        }
        self.drop_count(pane);
    }

    // A copy of tab i lands right after it and is shown, the way a new tab is.
    pub fn duplicate(&mut self, pane: &mut dyn Live, i: usize) {
        if self.count() >= MAX {
            self.say_count(pane, Say::Most);
            return;
        }
        if i >= self.count() {
            self.say_count(pane, Say::NoTab(i + 1));
            return;
        }
        let here = pane.resting_path();
        pane.close_preview();
        let dropped = pane.drop_overlay();
        let mut items = self.current_items(pane, Some(here.clone()));
        self.leave(pane, &mut items, &here, dropped);
        let mut copy = items[i].clone();
        copy.identity = self.identity();
        items.insert(i + 1, copy.clone());
        self.tabs = Some(pack(items, i + 1, None));
        self.drop_count(pane);
        self.apply(pane, &copy, dropped, None);
    }

    // A move lists nothing, so a listing out lands for the tab it belongs to with what its landing owes.
    pub fn move_tab(&mut self, pane: &mut dyn Live, from: usize, to: usize) {
        let here = pane.resting_path();
        let mut items = self.current_items(pane, Some(here.clone()));
        if items.len() < 2 {
            return;
        }
        let index = self.current();
        self.leave(pane, &mut items, &here, false);
        let next = super::bar::reorder(&mut items, from, to, index);
        let keep = self.tabs.as_ref().map(|tabs| tabs.pending.clone());
        self.tabs = Some(pack(items, next, keep.as_ref()));
    }

    pub fn move_current(&mut self, pane: &mut dyn Live, delta: isize) {
        let total = self.count();
        if total < 2 {
            return;
        }
        let from = self.current();
        self.move_tab(pane, from, super::bar::step(from, delta, total));
    }

    // The strip's key actions; false for an action that is not a tab's.
    pub fn act(&mut self, action: &str, pane: &mut dyn Live) -> bool {
        match action {
            "tabMoveLeft" => self.move_current(pane, -1),
            "tabMoveRight" => self.move_current(pane, 1),
            "tabNext" | "tabPrevious" => {
                let total = self.count();
                let next = if action == "tabNext" {
                    (self.current() + 1) % total
                } else {
                    (self.current() + total - 1) % total
                };
                self.select_at(pane, next);
            }
            "tabNew" => self.open_new(pane, None),
            "openTab" => self.open_cursor_tab(pane),
            "tabClose" => {
                let current = self.current();
                self.close_at(pane, current);
            }
            _ => match action.strip_prefix("tab").and_then(digit) {
                Some(number) => self.select_at(pane, number - 1),
                None => return false,
            },
        }
        true
    }

    // What a strip change writes; the write lands before the pane moves, so the current tab reads dropPath.
    pub fn remembered(&self, pane: &dyn Live) -> Plan {
        let mut here = pane.resting_path();
        let drop_path = pane.drop_path();
        if here == pane.path() && !drop_path.is_empty() {
            here = drop_path;
        }
        match &self.tabs {
            Some(tabs) if !tabs.items.is_empty() => Plan {
                paths: tabs
                    .items
                    .iter()
                    .enumerate()
                    .map(|(i, tab)| {
                        if i == tabs.index {
                            here.clone()
                        } else {
                            tab.path.clone()
                        }
                    })
                    .collect(),
                index: tabs.index,
            },
            _ => Plan {
                paths: vec![here],
                index: 0,
            },
        }
    }

    // A restored tab snapshots like a fresh tab, so its view, order and dotfile answer come from the pane.
    pub fn restore_items(&mut self, pane: &dyn Live, paths: &[String]) -> Vec<Tab> {
        paths
            .iter()
            .map(|path| self.snapshot(pane, Some(path.clone()), 0, false))
            .collect()
    }

    pub fn restore(&mut self, pane: &dyn Live, plan: &Plan) {
        let items = self.restore_items(pane, &plan.paths);
        self.tabs = Some(pack(items, plan.index, None));
    }
}

// "1" through "9" name a tab; anything longer or other is not a tab digit.
fn digit(text: &str) -> Option<usize> {
    match text.as_bytes() {
        [byte @ b'1'..=b'9'] => Some(usize::from(byte - b'0')),
        _ => None,
    }
}
