//! A tab moved between windows: its private payload, the receiver's checks, and the acknowledgment the source waits on.
use super::{
    bar::{insertion_at, Bar, Panes},
    state::{Live, Say, TabState, MAX},
};
use crate::navigation::folder_state::VIEWS;
use crate::navigation::sort::Snapshot;
use flea::jsondoc::{self, Json};
#[path = "payload.rs"]
mod wire;
pub use wire::{fresh_launch, parse, payload, payload_with_sort, tear_off_command, Launch};

// A lift offers only this private type, so no other application is ever offered the folder.
pub const TAB_MIME: &str = "application/x-flea-tab";

// The hand-off a tear-off gives the one window it starts; the core's tear-off list drops the same names.
pub const TEAR_OFF_ENV: [&str; 3] = ["FLEA_TAB_SOURCE_PID", "FLEA_TAB_CURSOR", "FLEA_TAB_TOKEN"];
pub const TAB_SORT_ENV: &str = "FLEA_TAB_SORT";

// An unacknowledged lift expires after one network leg's 15 s bound, and a receiver waits no longer.
pub const ACK_WAIT_MS: u64 = 15_000;

// The receiver's folder check asks this many rows, which tells its answer from any other peek.
pub const PEEK_FIRST: usize = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Info {
    pub pid: String,
    pub token: String,
    pub path: String,
    pub view: String,
    pub cursor: String,
    pub sort: Option<Snapshot>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Cancel,
    TearOff,
    Return(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Take,
    Ignore,
}

// One outstanding lift: the pane and identity it captured, since indices and paths move while its ack travels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lift {
    pub token: String,
    pub side: usize,
    pub identity: u64,
    // 0 while the drag is still live; the finish stamps it and starts the wait.
    pub lifted_at: u64,
    pub taken: bool,
}

// A received tab waiting on its folder check, tied to the pane that took the drop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingDrop {
    pub payload: String,
    pub pid: String,
    pub token: String,
    pub path: String,
    pub at: Option<usize>,
    pub side: usize,
}

// What a tear-off starts: the folder, and the hand-off naming the lift it answers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TearOff {
    pub pid: String,
    pub cursor: String,
    pub token: String,
    pub path: String,
    pub sort: Option<Snapshot>,
}

fn text(value: &Json) -> String {
    match value {
        Json::Str(text) => text.clone(),
        Json::Num(text) if text.parse::<f64>().is_ok_and(|n| n != 0.) => text.clone(),
        Json::Bool(true) => "true".into(),
        _ => String::new(),
    }
}

// An own-window drag belongs to reorder and must not duplicate its tab.
pub fn is_own(info: &Info, self_pid: &str) -> bool {
    !self_pid.is_empty() && info.pid == self_pid
}

pub fn has_tab_format(formats: &[String]) -> bool {
    formats.iter().any(|format| format == TAB_MIME)
}

// A foreign drag offers formats at enter with no bytes until the drop.
pub fn enter_accepts(
    formats: &[String],
    payload: &str,
    self_pid: &str,
    can_receive: bool,
    out_active: bool,
) -> bool {
    if payload.is_empty() {
        return has_tab_format(formats) && can_receive;
    }
    let Some(info) = parse(payload) else {
        return false;
    };
    if is_own(&info, self_pid) {
        return out_active;
    }
    can_receive
}

// The on-strip slot under x; off-strip drops never reach this.
pub fn drop_index_at(x: f64, tab_width: f64, count: usize) -> usize {
    insertion_at(x, tab_width, count)
}

// A drop's global point against the source window: outside tears off, on the strip returns, else cancels.
pub fn catcher_outcome(
    rect: Option<Rect>,
    strip: Option<Rect>,
    x: f64,
    y: f64,
    tab_width: f64,
    count: usize,
) -> Outcome {
    let Some(rect) = rect.filter(|rect| rect.width > 0. && rect.height > 0.) else {
        return Outcome::Cancel;
    };
    if !x.is_finite() || !y.is_finite() {
        return Outcome::Cancel;
    }
    let (local_x, local_y) = (x - rect.x, y - rect.y);
    if local_x < 0. || local_y < 0. || local_x >= rect.width || local_y >= rect.height {
        return Outcome::TearOff;
    }
    if let Some(strip) = strip {
        if local_x >= strip.x
            && local_x < strip.x + strip.width
            && local_y >= strip.y
            && local_y < strip.y + strip.height
        {
            return Outcome::Return(drop_index_at(local_x - strip.x, tab_width, count));
        }
    }
    Outcome::Cancel
}

// Only the outstanding lift's token may acknowledge that lift.
pub fn take_token(stored: &str, token: &str) -> bool {
    !stored.is_empty() && stored == token
}

pub fn ack_closes(stored: &str, token: &str, lifted_at: u64, now: u64) -> bool {
    take_token(stored, token) && !(lifted_at > 0 && now.saturating_sub(lifted_at) > ACK_WAIT_MS)
}

// The receiver's synchronous take at drop time; its folder check may still refuse, and then no ack goes out.
pub fn drop_decision(
    info: Option<&Info>,
    self_pid: &str,
    out_active: bool,
    can_receive: bool,
) -> Decision {
    let take = match info {
        None => false,
        Some(info) if is_own(info, self_pid) => out_active,
        Some(_) => can_receive,
    };
    if take {
        Decision::Take
    } else {
        Decision::Ignore
    }
}

// A loading listing or an open rename keeps its tab in the window.
pub fn tear_refusal(pane: &dyn Live) -> Option<Say> {
    if pane.busy() {
        return Some(Say::Busy);
    }
    pane.rename_pending().then_some(Say::FinishRename)
}

pub fn lift_for<'a>(lifts: &'a mut [Lift], token: &str) -> Option<&'a mut Lift> {
    lifts.iter_mut().find(|lift| take_token(&lift.token, token))
}

// One lift's token: the pid names the window, the token names the lift inside it.
pub fn new_token(now: u64, serial: u64) -> String {
    format!("{now}-{serial}")
}

// The four fields a peeked line echoes, which tell the folder check's answer from every other peek.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Peeked {
    pub path: String,
    pub hidden: bool,
    pub read_failed: bool,
    pub hidden_last: bool,
    pub first: usize,
}

// The folder check's request line: the reserved first, dotfiles off and last off.
pub fn peek_line(path: &str, first: usize, hidden: bool, hidden_last: bool) -> String {
    format!(
        "{{\"c\":\"peek\",\"path\":\"{}\",\"first\":{first},\"hidden\":{hidden},\"hiddenLast\":{hidden_last}}}",
        flea::json::escape(path)
    )
}

// Sample input: {"t":"peeked","path":"/tmp/a","n":0,"rows":[],"hidden":false,"hiddenLast":false,"first":2}.
pub fn peeked(document: &Json) -> Option<Peeked> {
    if document.get("t")?.as_str()? != "peeked" {
        return None;
    }
    let flag = |key| document.get(key).and_then(Json::as_bool) == Some(true);
    Some(Peeked {
        path: document.get("path")?.as_str()?.to_owned(),
        hidden: flag("hidden"),
        read_failed: flag("failed"),
        hidden_last: flag("hiddenLast"),
        first: document
            .get("first")
            .and_then(Json::as_f64)
            .filter(|first| *first >= 0. && first.fract() == 0.)? as usize,
    })
}

// Wayland hides a window's position from the window, so the source asks the compositor once per lift.
// Sample input: [{"pid":111,"at":[0,0],"size":[900,500],"mapped":true,"hidden":false}].
pub fn source_rect(clients: &str, pid: u32) -> Option<Rect> {
    let clients = jsondoc::parse(clients).ok()?;
    let own = pid.to_string();
    let matches: Vec<&Json> = clients
        .as_array()?
        .iter()
        .filter(|client| {
            client.get("pid").map(text).as_deref() == Some(own.as_str())
                && client.get("mapped").and_then(Json::as_bool) != Some(false)
                && client.get("hidden").and_then(Json::as_bool) != Some(true)
        })
        .collect();
    let [client] = matches.as_slice() else {
        return None;
    };
    let pair = |key| -> Option<(f64, f64)> {
        let values = client.get(key)?.as_array()?;
        Some((values.first()?.as_f64()?, values.get(1)?.as_f64()?))
    };
    let ((x, y), (width, height)) = (pair("at")?, pair("size")?);
    (x.is_finite() && y.is_finite() && width > 0. && height > 0.).then_some(Rect {
        x,
        y,
        width,
        height,
    })
}

// The compositor query a lift starts; an old query drains before the latest lift starts its own, and only its answer counts.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Geometry {
    pub token: String,
    pub asked: Option<String>,
    pub rect: Option<Rect>,
    pub strip: Option<Rect>,
}

impl Geometry {
    pub fn cancel(&mut self) {
        self.token.clear();
        self.rect = None;
        self.strip = None;
    }
    // True when the query should start now.
    pub fn begin(&mut self, token: &str, strip: Rect) -> bool {
        self.token = token.into();
        self.rect = None;
        self.strip = Some(strip);
        if self.asked.is_some() {
            return false;
        }
        self.asked = Some(token.into());
        true
    }

    // The query's answer; true when the latest lift still needs its own query.
    pub fn finished(&mut self, clients: &str, pid: u32) -> bool {
        let asked = self.asked.take().unwrap_or_default();
        if self.token.is_empty() {
            return false;
        }
        if asked == self.token {
            self.rect = source_rect(clients, pid);
            return false;
        }
        self.asked = Some(self.token.clone());
        true
    }
}

impl TabState {
    // A lift names the live current tab or a stored hidden one, with its cursor file when known.
    pub fn tab_info(
        &mut self,
        pane: &dyn Live,
        index: usize,
    ) -> Option<(String, &'static str, String)> {
        if index >= self.count() {
            return None;
        }
        let here = pane.resting_path();
        let (path, view, cursor) = if index == self.current() {
            let cursor = if here == pane.path() {
                super::state::cursor_file(pane)
            } else {
                String::new()
            };
            (here, pane.view(), cursor)
        } else {
            let items = self.current_items(pane, Some(here));
            let item = items.get(index)?;
            (item.path.clone(), item.view, item.cursor_name.clone())
        };
        path.starts_with('/').then_some((path, view, cursor))
    }

    pub fn tab_payload(&mut self, pane: &dyn Live, index: usize, pid: &str, token: &str) -> String {
        match self.tab_info(pane, index) {
            Some((path, view, cursor)) => {
                // A current tab the pane cannot snapshot carries its path and the pane's order, as other tabs do.
                let snapshot = if index == self.current() {
                    Some(
                        pane.sort_snapshot()
                            .filter(|snapshot| snapshot.path == path)
                            .unwrap_or_else(|| Snapshot {
                                path: path.clone(),
                                order: pane.order(),
                            }),
                    )
                } else {
                    self.tabs
                        .as_ref()
                        .and_then(|tabs| tabs.items.get(index))
                        .map(|tab| Snapshot {
                            path: tab.path.clone(),
                            order: tab.order,
                        })
                };
                snapshot.map_or_else(
                    || payload(pid, token, &path, view, &cursor),
                    |snapshot| payload_with_sort(pid, token, view, &cursor, &snapshot),
                )
            }
            None => String::new(),
        }
    }

    // A full strip refuses incoming tabs; a listing out is let go by the tab received.
    pub fn can_receive(&self) -> bool {
        self.count() < MAX
    }

    pub fn can_lift(&self) -> bool {
        self.count() >= 2
    }

    // A tab from another window opens at the requested slot and is shown.
    pub fn receive_tab(
        &mut self,
        pane: &mut dyn Live,
        payload: &str,
        at: Option<usize>,
        self_pid: &str,
    ) -> bool {
        let Some(info) = parse(payload).filter(|info| !is_own(info, self_pid)) else {
            return false;
        };
        if self.count() >= MAX {
            self.count_notice = Some(pane.say(Say::Most));
            return false;
        }
        pane.close_preview();
        let dropped = pane.drop_overlay();
        let here = pane.resting_path();
        let mut items = self.current_items(pane, Some(here.clone()));
        let index = self.current().min(items.len() - 1);
        let identity = items[index].identity;
        items[index] = self.snapshot(pane, Some(here), identity, dropped);
        let mut snap = self.snapshot(pane, Some(info.path.clone()), 0, dropped);
        if let Some(snapshot) = &info.sort {
            snap.order = snapshot.order;
        }
        if let Some(view) = VIEWS.iter().find(|view| **view == info.view) {
            snap.view = view;
        }
        if !info.cursor.is_empty() {
            snap.cursor_name = info.cursor.clone();
        }
        let place = at.filter(|&at| at <= items.len()).unwrap_or(items.len());
        items.insert(place, snap.clone());
        self.tabs = Some(super::state::pack(items, place, None));
        if let Some(serial) = self.count_notice.take() {
            pane.withdraw(serial);
        }
        self.apply(pane, &snap, dropped, Some(info.cursor));
        true
    }

    // Moving a tab out must never close the window's only remaining tab.
    pub fn close_after_move(&mut self, pane: &mut dyn Live, index: usize) -> bool {
        let total = self.count();
        if index >= total || total <= 1 {
            return false;
        }
        self.close_at(pane, index);
        true
    }

    // Tab lookup uses the live path for the current tab and stored paths for hidden ones.
    pub fn index_of_path(&mut self, pane: &dyn Live, path: &str) -> Option<usize> {
        if path.is_empty() {
            return None;
        }
        let here = pane.resting_path();
        let current = self.current();
        let items = self.current_items(pane, Some(here.clone()));
        items.iter().enumerate().position(|(i, item)| {
            if i == current {
                here == path
            } else {
                item.path == path
            }
        })
    }

    // A lift captures pane and identity; every tab gets one so a later ack can find it wherever it moved.
    pub fn capture_lift(
        &mut self,
        pane: &dyn Live,
        index: usize,
        token: &str,
        now: u64,
        side: usize,
    ) -> Option<Lift> {
        let mut items = self.current_items(pane, None);
        if index >= items.len() {
            return None;
        }
        for item in &mut items {
            if item.identity == 0 {
                item.identity = self.identity();
            }
        }
        let identity = items[index].identity;
        match self.tabs.as_mut() {
            Some(tabs) => tabs.items = items,
            None => self.tabs = Some(super::state::pack(items, 0, None)),
        }
        Some(Lift {
            token: token.into(),
            side,
            identity,
            lifted_at: now,
            taken: false,
        })
    }

    pub fn resolve(&self, identity: u64) -> Option<usize> {
        if identity == 0 {
            return None;
        }
        self.tabs
            .as_ref()?
            .items
            .iter()
            .position(|item| item.identity == identity)
    }
}

// The window's drag-out and receive half of the strip, ported from the shipped strip's own handlers.
impl Bar {
    // Resolve a lifted identity before using its cached index; closing that identity ends the gesture.
    pub fn resolve_drag(&mut self, panes: &mut dyn Panes) -> bool {
        let Some(lift) = self
            .lifts
            .iter()
            .find(|lift| lift.token == self.out_token && !lift.taken && lift.lifted_at == 0)
            .cloned()
        else {
            return false;
        };
        let mut resolved = None;
        let mut count = 0;
        panes.visit(lift.side, &mut |state, _| {
            resolved = state.resolve(lift.identity);
            count = state.count();
        });
        if let Some(index) = resolved.filter(|_| self.pane == Some(lift.side)) {
            self.drag_from = Some(index);
            self.drop_at = self.drop_at.map(|at| at.min(count));
            true
        } else {
            self.drag_from = None;
            self.drop_at = None;
            self.out_active = false;
            self.clear_ack();
            false
        }
    }

    // A lift captures the tab payload before the pane can change; a lone tab never lifts.
    pub fn tab_lift_began(&mut self, panes: &mut dyn Panes, index: usize, now: u64) {
        self.drag_cancelled();
        let Some(side) = self.pane else { return };
        let token = new_token(now, self.next_serial());
        let pid = self.self_pid.clone();
        let mut began = None;
        panes.visit(side, &mut |state, pane| {
            if !state.can_lift() {
                return;
            }
            let path = state
                .tab_info(pane, index)
                .map(|info| info.0)
                .unwrap_or_default();
            let Some(lift) = state.capture_lift(pane, index, &token, 0, side) else {
                return;
            };
            let payload = state.tab_payload(pane, index, &pid, &token);
            if payload.is_empty() {
                return;
            }
            began = Some((path, lift, payload));
        });
        let Some((path, lift, payload)) = began else {
            self.drag_from = None;
            self.drop_at = None;
            return;
        };
        self.drag_started(Some(index));
        self.out_index = Some(index);
        self.out_path = path;
        self.out_pid = pid;
        self.out_token = token;
        self.ack_lifted_at = 0;
        self.lifts.push(lift);
        self.out_payload = payload;
        self.out_outside = false;
        self.out_refused = false;
        self.own_accepted = false;
    }

    // A platform drag freezes the strip reorder until the drop answers; true when the platform drag starts now.
    pub fn tab_lift_moved(&mut self, panes: &mut dyn Panes, strip_x: f64, outside: bool) -> bool {
        if self.drag_from.is_none() || !self.resolve_drag(panes) {
            return false;
        }
        let count = self.count(panes);
        self.drag_moved(strip_x, count);
        self.out_outside = outside;
        if !outside || self.out_active || self.out_payload.is_empty() {
            return false;
        }
        let Some(side) = self.pane else { return false };
        let refused = self.out_refused;
        let mut refusal = None;
        panes.visit(side, &mut |_, pane| {
            refusal = tear_refusal(pane);
            if let Some(say) = refusal.clone().filter(|_| !refused) {
                pane.say(say);
            }
        });
        if refusal.is_some() {
            self.out_refused = true;
            return false;
        }
        self.out_active = true;
        true
    }

    // A platform gesture owns its release, so the strip's own release must not reorder again.
    pub fn tab_lift_ended(&mut self, panes: &mut dyn Panes, now: u64) {
        if self.out_active || self.own_accepted {
            self.drag_from = None;
            self.drop_at = None;
            return;
        }
        if !self.out_token.is_empty() && !self.resolve_drag(panes) {
            return;
        }
        self.drag_finished(panes, now);
        if self.ack_lifted_at == 0 {
            self.clear_ack();
        }
    }

    // The lift must survive the drag ending until its acknowledgment arrives.
    pub fn hold_ack(&mut self, now: u64) {
        self.ack_lifted_at = now.max(1);
        let stamp = self.ack_lifted_at;
        if let Some(lift) = lift_for(&mut self.lifts, &self.out_token.clone()) {
            lift.lifted_at = stamp;
        }
    }

    pub fn clear_ack(&mut self) {
        let token = std::mem::take(&mut self.out_token);
        self.lifts.retain(|lift| lift.token != token);
        self.out_index = None;
        self.out_path.clear();
        self.out_payload.clear();
        self.ack_lifted_at = 0;
    }

    // Expired lifts go, and a taken lift closes its tab once its captured pane is not loading.
    pub fn drain_lifts(&mut self, panes: &mut dyn Panes, now: u64) {
        if self.drag_from.is_some() {
            return;
        }
        let mut kept = Vec::new();
        for lift in std::mem::take(&mut self.lifts) {
            if !lift.taken && !ack_closes(&lift.token, &lift.token, lift.lifted_at, now) {
                if self.out_token == lift.token {
                    self.out_token.clear();
                }
                continue;
            }
            let mut keep = false;
            let mut closed = false;
            let found = panes.visit(lift.side, &mut |state, pane| {
                let Some(index) = state.resolve(lift.identity) else {
                    return;
                };
                if lift.taken && !pane.busy() {
                    state.close_after_move(pane, index);
                    closed = true;
                } else {
                    keep = true;
                }
            });
            if closed && self.out_token == lift.token {
                self.out_token.clear();
            }
            if found && keep {
                kept.push(lift);
            }
        }
        self.lifts = kept;
    }

    // A cross-process drop closes the source tab only after a matching acknowledgment.
    pub fn out_finished(&mut self, now: u64) {
        let consumed = self.own_accepted;
        self.drag_from = None;
        self.drop_at = None;
        self.out_active = false;
        self.own_accepted = false;
        // Only the identity-bearing lift survives completion, waiting for its acknowledgment.
        self.out_payload.clear();
        if consumed {
            self.clear_ack();
        } else if !self.out_token.is_empty() {
            self.hold_ack(now);
        }
    }

    // Escape and an explicit cancel share the same ending.
    pub fn cancel_out(&mut self, now: u64) {
        if self.out_active {
            self.out_finished(now);
        }
    }

    pub fn return_at(&mut self, panes: &mut dyn Panes, at: usize) {
        let token = self.out_token.clone();
        if let Some(lift) = lift_for(&mut self.lifts, &token).cloned() {
            panes.visit(lift.side, &mut |state, pane| {
                if let Some(from) = state.resolve(lift.identity) {
                    state.move_tab(pane, from, if at > from { at - 1 } else { at });
                }
            });
        }
        self.own_accepted = true;
    }

    // The drop's global point decides between a tear-off, a return to the strip and a cancel.
    pub fn catcher_drop(
        &mut self,
        panes: &mut dyn Panes,
        rect: Option<Rect>,
        strip: Option<Rect>,
        x: f64,
        y: f64,
        now: u64,
    ) -> Option<TearOff> {
        if !self.out_active {
            return None;
        }
        let count = self.count(panes);
        let outcome = catcher_outcome(rect, strip, x, y, f64::from(self.tab_width(count)), count);
        super::trace::stage("catcher-drop", || {
            let name = match outcome {
                Outcome::TearOff => "tearoff",
                Outcome::Return(_) => "return",
                Outcome::Cancel => "cancel",
            };
            let unknown = if rect.is_some() {
                ""
            } else {
                " reason=unknown-source-rectangle"
            };
            format!("outcome={name} global={x},{y}{unknown}")
        });
        match outcome {
            Outcome::TearOff => self.tear_off_at(now),
            Outcome::Return(at) => {
                self.return_at(panes, at);
                None
            }
            Outcome::Cancel => None,
        }
    }

    // The new window acknowledges only after listing; the caller spawns what this answers.
    pub fn tear_off_at(&mut self, now: u64) -> Option<TearOff> {
        if self.out_path.is_empty() {
            return None;
        }
        let cursor = parse(&self.out_payload)
            .map(|info| info.cursor)
            .unwrap_or_default();
        let tear = TearOff {
            pid: self.out_pid.clone(),
            cursor,
            token: self.out_token.clone(),
            path: self.out_path.clone(),
            sort: parse(&self.out_payload).and_then(|info| info.sort),
        };
        self.out_finished(now);
        Some(tear)
    }

    // An own-window tab drag reorders its strip instead of opening another tab.
    pub fn tab_enter_ok(&self, panes: &mut dyn Panes, formats: &[String], payload: &str) -> bool {
        let can = self.can_receive(panes);
        enter_accepts(formats, payload, &self.self_pid, can, self.out_active)
    }

    pub fn can_receive(&self, panes: &mut dyn Panes) -> bool {
        let Some(side) = self.pane else { return false };
        let mut can = false;
        panes.visit(side, &mut |state, _| can = state.can_receive());
        can
    }

    // A strip drop: an own lift returns and reorders, a foreign tab waits on its folder check; true when the drop is taken.
    pub fn strip_drop(&mut self, panes: &mut dyn Panes, payload: &str, x: f64, now: u64) -> bool {
        let Some(info) = parse(payload) else {
            return false;
        };
        let count = self.count(panes);
        let at = drop_index_at(x, f64::from(self.tab_width(count)), count);
        if is_own(&info, &self.self_pid) {
            if !self.out_active || info.token != self.out_token {
                return false;
            }
            self.return_at(panes, at);
            return true;
        }
        let can = self.can_receive(panes);
        if drop_decision(Some(&info), &self.self_pid, self.out_active, can) != Decision::Take {
            return false;
        }
        // The take answers Move at once; the folder check behind it may still refuse, and then no ack goes out.
        self.accept_tab_drop(panes, payload, &info, Some(at), now)
    }

    // A foreign tab opens only after its reserved peek confirms a folder; one drop waits at a time.
    pub fn accept_tab_drop(
        &mut self,
        panes: &mut dyn Panes,
        payload: &str,
        info: &Info,
        at: Option<usize>,
        now: u64,
    ) -> bool {
        let Some(side) = self.pane else {
            return false;
        };
        if self.pending.is_some() || !self.can_receive(panes) {
            return false;
        }
        self.pending = Some(PendingDrop {
            payload: payload.into(),
            pid: info.pid.clone(),
            token: info.token.clone(),
            path: info.path.clone(),
            at,
            side,
        });
        self.deadline = Some(now + ACK_WAIT_MS);
        panes.visit(side, &mut |_, pane| {
            pane.peek(&info.path, PEEK_FIRST, false, false)
        });
        true
    }

    // A pane change, the deadline or a lost pane refuses the drop out loud, on the pane that will hear it.
    pub fn refuse_tab_drop(&mut self, panes: &mut dyn Panes) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        self.deadline = None;
        let side = self.pane.unwrap_or(pending.side);
        panes.visit(side, &mut |_, pane| {
            pane.say(Say::NotReceived);
        });
    }

    pub fn set_pane(&mut self, panes: &mut dyn Panes, side: Option<usize>) {
        if self.pane == side {
            return;
        }
        let captured = self.pending.as_ref().map(|pending| pending.side);
        self.pane = side;
        if side.is_none() {
            // The bar lost its pane, so the refusal lands where the drop was taken.
            if let Some(pending) = self.pending.take() {
                self.deadline = None;
                panes.visit(captured.unwrap_or(pending.side), &mut |_, pane| {
                    pane.say(Say::NotReceived);
                });
            }
            return;
        }
        self.refuse_tab_drop(panes);
    }

    // The deadline timer's tick; true when it refused a drop.
    pub fn expire(&mut self, panes: &mut dyn Panes, now: u64) -> bool {
        match self.deadline {
            Some(at) if now >= at => {
                self.refuse_tab_drop(panes);
                true
            }
            _ => false,
        }
    }

    // A peek answer from side's backend; only the pending drop's own pane and discriminator settle it.
    #[allow(clippy::too_many_arguments)]
    pub fn on_peeked(
        &mut self,
        panes: &mut dyn Panes,
        side: usize,
        path: &str,
        hidden: bool,
        read_failed: bool,
        hidden_last: bool,
        first: usize,
    ) {
        if self.pane != Some(side) {
            return;
        }
        let Some(pending) = self.pending.clone() else {
            return;
        };
        if path != pending.path || hidden || hidden_last || first != PEEK_FIRST {
            return;
        }
        self.pending = None;
        self.deadline = None;
        let pid = self.self_pid.clone();
        let mut received = false;
        panes.visit(side, &mut |state, pane| {
            if read_failed {
                pane.say(Say::FolderGone);
                return;
            }
            received = state.receive_tab(pane, &pending.payload, pending.at, &pid);
        });
        if received {
            self.send_taken(&pending.pid, &pending.token);
        }
    }

    // Sends the taken acknowledgment only after the receiving tab opens; acks queue one at a time.
    pub fn send_taken(&mut self, pid: &str, token: &str) {
        self.taken_queue.push_back((pid.into(), token.into()));
    }

    // The next acknowledgment to run, if none is running.
    pub fn pump_taken(&mut self) -> Option<(String, String)> {
        if self.ack_running {
            return None;
        }
        let next = self.taken_queue.pop_front()?;
        self.ack_running = true;
        Some(next)
    }

    pub fn taken_exited(&mut self) -> Option<(String, String)> {
        self.ack_running = false;
        self.pump_taken()
    }

    // The source's fleatab.taken: only a live lift's matching token closes its captured tab.
    pub fn take(&mut self, panes: &mut dyn Panes, token: &str, now: u64) -> bool {
        let Some(lift) = lift_for(&mut self.lifts, token) else {
            return false;
        };
        if !ack_closes(&lift.token, token, lift.lifted_at, now) {
            return false;
        }
        let (side, identity) = (lift.side, lift.identity);
        let mut resolved = false;
        panes.visit(side, &mut |state, _| {
            resolved = state.resolve(identity).is_some()
        });
        if !resolved {
            return false;
        }
        if let Some(lift) = lift_for(&mut self.lifts, token) {
            lift.taken = true;
        }
        self.drain_lifts(panes, now);
        true
    }
}
