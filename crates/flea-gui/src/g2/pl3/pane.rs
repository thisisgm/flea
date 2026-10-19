//! The Trash pane entity: the core session, its seams, its settle timers and the paint state its Elements read.
use super::board::{HEADER_H, MENU_DROP, PAD_X, ROW_H, STRIP_H};
use super::ink::Ink;
use super::list::{Line, ListPaint};
use super::monitor::{Heard, Monitor, Started};
use super::seams::{At, Card, Outside};
use crate::row::marks::Mark;
use flea::i18n::I18n;
use flea::jsondoc::{self, Json};
use flea::ui_logic_pl3::host::{self, Host};
use flea::ui_logic_pl3::rows::Reply;
use flea::ui_logic_pl3::view::{Anchor, Keys, Press, Signal, View, CHECK_MS};
use flea::ui_logic_pl3::words::{self, Scope, Snapshot};
use gpui::{Context, FocusHandle, ImageAssetLoader, Resource, Task, Window};
use std::{cell::RefCell, collections::HashSet, path::Path, rc::Rc, sync::Arc, time::{Duration, Instant}};

/// Empty Trash…'s pointer state; keyboard focus is the view's strip_focus.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Button {
    pub hover: bool,
    pub pressed: bool,
}

pub struct TrashPane {
    pub host: Host,
    pub catalog: Arc<I18n>,
    pub outside: Box<dyn Outside>,
    pub paint: Rc<RefCell<ListPaint>>,
    pub focus: FocusHandle,
    pub button: Button,
    pub menu_open: bool,
    pub width: f32,
    /// Menu rows hidden by the menu settings; Delete permanently is hidden by default.
    pub hidden: Vec<String>,
    keys: (String, String),
    home: String,
    /// The review op1 shows; an answer from any other card is stale and dropped.
    card: Option<Snapshot>,
    // Only the newest settle generation can ask, so a new timer replaces and cancels the old one.
    timer: Option<Task<()>>,
    // TrashView's 2500 ms check, armed while a card is open and nothing is in flight, so it restarts after each reply.
    checker: Option<Task<()>>,
    monitor: Option<(Monitor, Task<()>)>,
    decoded: HashSet<Resource>,
    pub(super) retired: Vec<Resource>,
    epoch: Instant,
    /// Starts the Trash watch when Trash opens; None leaves it to a host that already watches.
    pub monitor_start: Option<fn() -> Started>,
}

impl TrashPane {
    pub fn new(catalog: Arc<I18n>, outside: Box<dyn Outside>, ink: Ink, preset: &str, home: String, cx: &mut Context<Self>) -> Self {
        let typeface = crate::list::Typeface::omarchy_default();
        Self {
            host: Host::new(View::new(ROW_H)),
            catalog,
            outside,
            paint: Rc::new(RefCell::new(ListPaint::new(typeface.font, typeface.body, ink))),
            focus: cx.focus_handle(),
            button: Button::default(),
            menu_open: false,
            width: 0.,
            hidden: vec!["delete".into()],
            keys: words::prompt_keys(preset),
            home,
            card: None,
            timer: None,
            checker: None,
            monitor: None,
            decoded: HashSet::new(),
            retired: Vec::new(),
            epoch: Instant::now(),
            monitor_start: Some(Monitor::start),
        }
    }

    fn now(&self) -> i64 {
        self.epoch.elapsed().as_millis() as i64
    }
    pub fn view(&self) -> &View {
        &self.host.view
    }
    /// Empty Trash… is live only with something to empty and nothing in flight, as the menu row is.
    pub fn empty_available(&self) -> bool {
        self.host.view.total > 0 && !self.host.view.busy() && !self.host.view.operation_active
    }

    pub fn open(&mut self, action: &str, cx: &mut Context<Self>) {
        self.watch(cx);
        self.host.open(action);
        self.pump(cx);
    }

    // The watch outlives a close, as the rail's TrashMonitor does; it starts with the first open.
    fn watch(&mut self, cx: &mut Context<Self>) {
        let Some(start) = self.monitor_start.filter(|_| self.monitor.is_none()) else { return };
        match start() {
            Ok((monitor, heard)) => {
                let listener = cx.spawn(async move |this, cx| {
                    while let Ok(message) = heard.recv().await {
                        if this.update(cx, |pane, cx| pane.heard(message, cx)).is_err() {
                            break;
                        }
                    }
                });
                self.monitor = Some((monitor, listener));
            }
            Err(_) => self.outside.status(self.catalog.tr("trash.monitor.watch_failed"), true),
        }
    }
    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.host.close();
        self.pump(cx);
    }
    pub fn resize(&mut self, width: f32, height: f32, cx: &mut Context<Self>) {
        self.width = width;
        let rows = (height - STRIP_H - HEADER_H).max(0.);
        if rows != self.host.view.height {
            let y = self.host.view.scroll_y;
            self.host.view.set_viewport(y, rows);
            self.pump(cx);
        }
    }

    /// What the Trash watch heard: the rail's count, a failure, or a settled burst an open Trash re-reads.
    pub fn heard(&mut self, heard: Heard, cx: &mut Context<Self>) {
        match heard {
            Heard::Count(total) => self.outside.count(total as usize),
            Heard::Failed(key) => self.outside.status(self.catalog.tr(key), true),
            Heard::Changed => {
                self.host.view.source_changed();
                self.pump(cx);
            }
        }
    }

    /// One backend line on this session: a trashbrowse reply or a trashthumbed answer.
    pub fn line(&mut self, text: &str, cx: &mut Context<Self>) {
        let Ok(message) = jsondoc::parse(text) else { return };
        match message.get("t").and_then(Json::as_str) {
            Some("trashbrowse") => {
                if let Some(reply) = Reply::read(message) {
                    let now = self.now();
                    let _ = self.host.receive(&reply, &self.catalog, now);
                }
            }
            Some("trashthumbed") => {
                let id = message.get("id").and_then(Json::as_f64).unwrap_or(0.) as u64;
                let file = message.get("file").and_then(Json::as_str).unwrap_or("");
                self.host.view.thumbnailed(id, file);
            }
            _ => return,
        }
        self.pump(cx);
    }

    /// The strip's back button: TrashView's root.close(), live through a review as the oracle's is.
    pub fn back_pressed(&mut self, cx: &mut Context<Self>) {
        if self.host.view.opened {
            self.host.view.close();
        }
        self.pump(cx);
    }

    /// A key action from the generated table, while Trash holds the keyboard.
    pub fn action(&mut self, name: &str, repeat: bool, cx: &mut Context<Self>) {
        if self.host.view.strip_focus && matches!(name, "open" | "activate") {
            if self.empty_available() {
                self.host.view.prepare(true);
            }
            return self.pump(cx);
        }
        if name == "escape" && self.host.view.confirmation.is_none() && self.outside.escape_taken() {
            self.host.view.arm.clear();
            return self.pump(cx);
        }
        let (now, keys) = (self.now(), Keys { arm: &self.keys.0, alone: &self.keys.1 });
        let press = match name {
            "parent" | "back" => Press::Backspace,
            "reload" => Press::F5,
            other => Press::Action(other),
        };
        self.host.view.press(press, repeat, now, &self.catalog, keys);
        self.pump(cx);
    }

    /// Tab moves the keyboard between the list and Empty Trash…, the strip's one live stop.
    pub fn focus_strip(&mut self, on: bool, cx: &mut Context<Self>) {
        self.host.view.strip_focus = on && self.empty_available();
        self.pump(cx);
    }
    pub fn empty_pressed(&mut self, cx: &mut Context<Self>) {
        if self.empty_available() {
            self.host.action("emptyTrash");
        }
        self.pump(cx);
    }

    pub fn click_row(&mut self, index: usize, toggle: bool, cx: &mut Context<Self>) {
        let extend = if toggle { flea::ui_logic_pl3::selection::Extend::Toggle } else { flea::ui_logic_pl3::selection::Extend::No };
        self.host.view.strip_focus = false;
        self.host.view.choose(index as isize, extend);
        self.pump(cx);
    }
    pub fn right_click(&mut self, index: Option<usize>, x: f32, y: f32, cx: &mut Context<Self>) {
        let view = &mut self.host.view;
        if let Some(index) = index {
            let marked = view.row_at(index).is_some_and(|row| view.is_selected(&row.uri));
            if !marked {
                view.choose(index as isize, flea::ui_logic_pl3::selection::Extend::No);
            }
        }
        let selection = index.is_some() || view.selected_count() > 0;
        self.open_menu(Anchor::Point(x, y), selection);
        self.pump(cx);
    }
    pub fn hover(&mut self, index: Option<usize>, cx: &mut Context<Self>) {
        if self.paint.borrow().hover != index {
            self.paint.borrow_mut().hover = index;
            cx.notify();
        }
    }
    pub fn wheel(&mut self, dy: f32, cx: &mut Context<Self>) {
        let (y, h) = (self.host.view.scroll_y + dy, self.host.view.height);
        self.host.view.set_viewport(y, h);
        self.pump(cx);
    }

    fn open_menu(&mut self, at: Anchor, selection: bool) {
        let view = &self.host.view;
        let items = if selection { host::selection_menu(view.busy(), &self.hidden) } else { host::trash_menu(view.total, view.busy()) };
        let at = match at {
            Anchor::Row(index) => At { x: PAD_X, y: STRIP_H + HEADER_H + (index + 1) as f32 * ROW_H - view.scroll_y + MENU_DROP },
            Anchor::Top => At { x: self.width / 2., y: STRIP_H },
            Anchor::Point(x, y) => At { x, y },
        };
        self.menu_open = true;
        self.outside.menu(items, at);
    }
    /// The menu primitive closed, with the picked row's action or none.
    pub fn menu_action(&mut self, action: Option<&str>, cx: &mut Context<Self>) {
        self.menu_open = false;
        match action {
            Some("open") => self.host.open(""),
            Some(action) => self.host.action(action),
            None => {}
        }
        self.pump(cx);
    }
    /// op1 answered the card: the destructive button's token, or None for Cancel and Escape.
    pub fn card_answered(&mut self, token: Option<u64>, cx: &mut Context<Self>) {
        let Some(shown) = self.card.as_ref().map(|card| card.token) else { return };
        if token.is_some_and(|token| token != shown) {
            return;
        }
        self.card = None;
        self.host.view.confirmed(token);
        self.pump(cx);
    }

    /// Sends what the session queued, hands every signal to its seam and refreshes the paint inputs.
    pub fn pump(&mut self, cx: &mut Context<Self>) {
        for line in self.host.drain() {
            self.outside.send(line);
        }
        for signal in self.host.view.signals() {
            match signal {
                Signal::Settle { generation, delay_ms } => self.settle(generation, delay_ms, cx),
                Signal::Status { text, error } => self.outside.status(text, error),
                Signal::Arm(text) => self.outside.status(text, false),
                Signal::Result { headline, detail, error } => self.outside.result(headline, detail, error),
                Signal::Count(total) => self.outside.count(total),
                Signal::Back => self.outside.back(),
                Signal::FocusRail => self.outside.focus_rail(),
                // TrashHost.onActionRequested drops forwarded actions for the whole review.
                Signal::Action(name) if !self.host.view.confirming => self.outside.action(name),
                Signal::Action(_) => {}
                Signal::Context { at, selection } => self.open_menu(at, selection),
            }
        }
        self.sync_card();
        self.sync_check(cx);
        self.sync_paint();
        cx.notify();
    }

    fn sync_check(&mut self, cx: &mut Context<Self>) {
        if self.card.is_none() || self.host.view.busy() {
            self.checker = None;
        } else if self.checker.is_none() {
            self.checker = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(Duration::from_millis(CHECK_MS)).await;
                let _ = this.update(cx, |pane, cx| {
                    pane.checker = None;
                    pane.host.view.check();
                    pane.pump(cx);
                });
            }));
        }
    }

    fn settle(&mut self, generation: u64, delay_ms: u64, cx: &mut Context<Self>) {
        self.timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(delay_ms)).await;
            let _ = this.update(cx, |pane, cx| {
                pane.host.view.settled(generation);
                pane.pump(cx);
            });
        }));
    }

    // One card per review: a dropped or changed review closes the card on screen before the new one opens, as sourceChanged does.
    fn sync_card(&mut self) {
        let view = &self.host.view;
        if view.confirmation == self.card {
            return;
        }
        if self.card.take().is_some() {
            self.outside.card(None);
        }
        let Some(snapshot) = view.confirmation.clone() else { return };
        let scope = if snapshot.all { Scope::Trash } else { Scope::Items };
        let card = Card {
            title: words::confirm_title(&self.catalog, &snapshot),
            body: words::confirm_body(&self.catalog, &snapshot, scope),
            confirm: self.catalog.tr(if snapshot.all { "trash.confirm.empty_button" } else { "trash.confirm.delete_button" }),
            cancel: self.catalog.tr("trash.confirm.cancel"),
            token: snapshot.token,
            empty: snapshot.all,
        };
        self.card = Some(snapshot);
        self.outside.card(Some(card));
    }

    fn sync_paint(&mut self) {
        let view = &self.host.view;
        let mut paint = self.paint.borrow_mut();
        let lines: Vec<Line> = view.rows.iter().enumerate().map(|(at, row)| Line {
            name: row.name().to_string(),
            location: words::location_text(&self.catalog, &row.original, &self.home),
            deleted: words::deleted_text(&self.catalog, row.deleted.as_deref()),
            mark: if row.directory { Mark::Folder } else { Mark::for_row(&row.icon, 0o100644) },
            marked: view.is_selected(&row.uri),
            thumb: Some(view.thumbs.file_for(view.first + at, view.policy)).filter(|file| !file.is_empty())
                .map(|file| Resource::Path(Arc::from(Path::new(file)))),
        }).collect();
        let same_text = lines.len() == paint.lines.len()
            && lines.iter().zip(&paint.lines).all(|(a, b)| a.name == b.name && a.location == b.location && a.deleted == b.deleted);
        if !same_text || paint.first != view.first {
            paint.revision += 1;
        }
        let live: HashSet<Resource> = lines.iter().filter_map(|line| line.thumb.clone()).collect();
        self.retired.extend(self.decoded.difference(&live).cloned());
        self.decoded = live;
        paint.lines = lines;
        paint.first = view.first;
        paint.total = view.total;
        paint.scroll_y = view.scroll_y;
        paint.cursor = view.cursor;
        paint.edge = !view.strip_focus && !self.menu_open && self.card.is_none();
    }

    /// Images no row draws any more leave GPUI's asset cache and the sprite atlas, so memory follows the viewport.
    pub(super) fn evict(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for source in std::mem::take(&mut self.retired) {
            if let Some(Ok(image)) = window.get_asset::<ImageAssetLoader>(&source, cx) {
                cx.drop_image(image, Some(window));
            }
            cx.remove_asset::<ImageAssetLoader>(&source);
        }
    }
}
