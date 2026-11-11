//! TrashHost.qml: the one Trash session's owner, routing actions to the view and running the opt-in 30 day sweep.
use super::dates;
use super::rows::{Line, Reply, TrashRow};
use super::view::View;
use super::words::{self, Route};
use crate::i18n::I18n;

pub const SWEEP_DAYS: i64 = 30;
/// One listing window, the most the backend answers with; a larger Trash is swept over more days.
pub const SWEEP_WINDOW: usize = 350;

/// A menu row the menu primitive draws; labels resolve through the catalog when drawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub id: &'static str,
    pub action: &'static str,
    pub label_key: &'static str,
    pub glyph: &'static str,
    pub disabled: bool,
    pub danger: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    Entry(Entry),
    Separator,
}

fn entry(id: &'static str, action: &'static str, label_key: &'static str, glyph: &'static str) -> Entry {
    Entry { id, action, label_key, glyph, disabled: false, danger: false }
}

/// Menu.applyHidden: hidden rows leave, and no separator leads, doubles or trails.
fn apply_hidden(items: Vec<Item>, hidden: &[String]) -> Vec<Item> {
    let mut kept: Vec<Item> = Vec::new();
    for item in items {
        match item {
            Item::Separator if matches!(kept.last(), Some(Item::Entry(_))) => kept.push(item),
            Item::Separator => {}
            Item::Entry(e) if !hidden.iter().any(|h| h == e.id) => kept.push(Item::Entry(e)),
            Item::Entry(_) => {}
        }
    }
    while matches!(kept.last(), Some(Item::Separator)) {
        kept.pop();
    }
    kept
}

/// The marked or cursor rows' menu; Delete permanently follows the menu's hidden set, off by default.
pub fn selection_menu(busy: bool, hidden: &[String]) -> Vec<Item> {
    let restore = Entry { disabled: busy, ..entry("restoreTrashSelection", "restoreTrashSelection", "trash.menu.restore", "undo") };
    let delete = Entry { disabled: busy, danger: true, ..entry("delete", "deletePermanently", "trash.menu.delete", "trash") };
    apply_hidden(vec![Item::Entry(restore), Item::Separator, Item::Entry(delete)], hidden)
}

/// The whole-Trash menu, in the rail and on the strip: Open, Restore All, then Empty Trash… last in the destructive ink.
pub fn trash_menu(total: usize, busy: bool) -> Vec<Item> {
    let off = total == 0 || busy;
    vec![
        Item::Entry(entry("open", "open", "trash.menu.open", "folder-open")),
        Item::Separator,
        Item::Entry(Entry { disabled: off, ..entry("restoreAll", "restoreAll", "trash.menu.restore_all", "undo") }),
        Item::Separator,
        Item::Entry(Entry { disabled: off, danger: true, ..entry("emptyTrash", "emptyTrash", "trash.menu.empty", "trash") }),
    ]
}

/// The keys sheet's menu: the selection rows, disabled with nothing marked, then the whole-Trash rows.
pub fn sheet_menu(selected: usize, total: usize, busy: bool) -> Vec<Item> {
    let mut rows = selection_menu(busy, &[]);
    if selected == 0 {
        for item in &mut rows {
            if let Item::Entry(e) = item {
                e.disabled = true;
            }
        }
    }
    rows.extend(trash_menu(total, busy));
    rows
}

/// The listing keys Trash answers itself, so its keys sheet lists them beside the window keys it forwards.
pub const OWN_SHEET_KEYS: &[&str] = &[
    "cursorDown", "cursorUp", "cursorFirst", "cursorLast", "pageDown", "pageUp", "focusNext", "focusPrevious", "escape",
    "reload", "deletePermanently", "toggleSelect", "selectAll", "extendDown", "extendUp", "menu",
];

/// The keys sheet lists the window actions Trash forwards and the listing keys it answers itself.
pub fn sheet_action(action: &str) -> bool {
    words::route(action) != Route::Trash || action == "quit" || OWN_SHEET_KEYS.contains(&action)
}

/// How a sweep reply ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Swept {
    Continue,
    /// Record this local day so the sweep runs once a day; refresh the rail count when items were deleted.
    Finished { day: i64, deleted: bool },
    /// A failure at any stage records nothing, so the next launch tries again.
    Failed,
}

#[derive(Debug, Default)]
pub struct Host {
    pub view: View,
    sweep_id: u64,
    stage: String,
    doomed: Vec<TrashRow>,
    out: Vec<String>,
}

impl Host {
    pub fn new(view: View) -> Self {
        Self { view, ..Self::default() }
    }
    pub fn sweeping(&self) -> bool {
        !self.stage.is_empty()
    }
    pub fn sweep_request(&self) -> Option<(u64, &str)> {
        self.sweeping().then_some((self.sweep_id, self.stage.as_str()))
    }
    /// Sweep lines first, then the view's, in send order.
    pub fn drain(&mut self) -> Vec<String> {
        let mut out = std::mem::take(&mut self.out);
        out.extend(self.view.drain());
        out
    }

    fn sweep_send(&mut self, op: &str, line: Line) {
        self.sweep_id += 1;
        self.stage = op.into();
        self.out.push(line.text("op", op).number("id", self.sweep_id).finish());
    }

    /// The window and the sweep share one backend session, so a sweep starts only while the view is closed.
    pub fn sweep(&mut self, now_ms: i64, enabled: bool, swept_on: i64) {
        if self.sweeping() || self.view.opened || self.view.confirming || self.view.operation_active || self.view.busy()
            || !enabled || dates::day_number(now_ms) == swept_on {
            return;
        }
        self.sweep_id = self.sweep_id.max(self.view.request_id);
        self.sweep_send("list", Line::new("trashbrowse").number("start", 0).number("count", SWEEP_WINDOW).flag("recover", false));
    }

    /// The operator reaching for Trash outranks a sweep; cancelling drops its prepared review.
    pub fn sweep_stop(&mut self) {
        if !self.sweeping() {
            return;
        }
        self.stage.clear();
        self.doomed.clear();
        self.sweep_id += 1;
        self.out.push(Line::new("trashbrowse").text("op", "cancel").number("id", self.sweep_id).flag("clearSelection", true).finish());
    }

    fn sweep_receive(&mut self, reply: &Reply, now_ms: i64) -> Swept {
        let finished = |host: &mut Self, deleted| {
            host.stage.clear();
            host.doomed.clear();
            Swept::Finished { day: dates::day_number(now_ms), deleted }
        };
        if !reply.ok {
            self.stage.clear();
            self.doomed.clear();
            return Swept::Failed;
        }
        match reply.op.as_str() {
            "list" => {
                let rows = reply.rows().unwrap_or_default();
                self.doomed = rows.into_iter().filter(|row| dates::expired(row.deleted.as_deref(), now_ms, SWEEP_DAYS)).collect();
                if self.doomed.is_empty() {
                    return finished(self, false);
                }
                let uris: Vec<String> = self.doomed.iter().map(|row| row.uri.clone()).collect();
                let identities: Vec<String> = self.doomed.iter().map(|row| row.identity.clone()).collect();
                let line = Line::new("trashbrowse").flag("all", false).flag("emptyTrash", false).texts("uris", &uris).texts("identities", &identities);
                self.sweep_send("prepare", line);
                Swept::Continue
            }
            "prepare" => match super::rows::count(&reply.body, "count").filter(|count| *count > 0) {
                None => finished(self, false),
                Some(_) => {
                    let token = super::rows::count(&reply.body, "token").unwrap_or(0);
                    self.sweep_send("delete", Line::new("trashbrowse").number("token", token));
                    Swept::Continue
                }
            },
            "delete" => finished(self, true),
            _ => Swept::Continue,
        }
    }

    /// Every reply while a sweep is in flight is the sweep's, because the view cannot be open then.
    pub fn receive(&mut self, reply: &Reply, i18n: &I18n, now_ms: i64) -> Option<Swept> {
        if self.sweeping() {
            if reply.id != self.sweep_id || reply.op != self.stage {
                return Some(Swept::Continue);
            }
            return Some(self.sweep_receive(reply, now_ms));
        }
        self.view.receive(reply, i18n);
        None
    }

    pub fn open(&mut self, action: &str) {
        if self.view.confirming {
            return;
        }
        self.sweep_stop();
        self.view.request_id = self.view.request_id.max(self.sweep_id);
        if self.view.opened {
            return self.action(if action.is_empty() { "openTrash" } else { action });
        }
        self.view.open(action);
    }

    /// Review every current item from the sidebar while keeping a closed view closed.
    pub fn empty_from_rail(&mut self) {
        if self.view.opened {
            return self.action("emptyTrash");
        }
        if self.view.confirming || self.view.operation_active {
            return;
        }
        self.sweep_stop();
        self.view.request_id = self.view.request_id.max(self.sweep_id);
        self.view.selection.clear();
        self.view.prepare_review(true);
    }

    pub fn close(&mut self) {
        if self.view.opened && !self.view.confirming {
            self.view.close();
        }
    }

    /// A menu or key action addressed to Trash; an unopened view opens with it as its first action.
    pub fn action(&mut self, name: &str) {
        if self.view.confirming {
            return;
        }
        if !self.view.opened {
            return self.open(name);
        }
        match name {
            "restoreTrashSelection" => self.view.restore(false),
            "deletePermanently" => self.view.prepare(false),
            "restoreAll" => self.view.restore(true),
            "emptyTrash" => self.view.prepare(true),
            _ => {}
        }
    }
}
