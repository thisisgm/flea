//! The Trash pane entity: the core session, its seams, its settle timers and the paint state its Elements read.
use super::board::Board;
use super::ink::Ink;
use super::list::ListPaint;
use super::monitor::{Heard, Monitor, Started};
#[path = "connection.rs"]
mod connection;
#[path = "pane_input.rs"]
mod input;
#[path = "paint_sync.rs"]
mod paint_sync;
use super::seams::{At, Card, Outside};
use flea::i18n::I18n;
use flea::jsondoc::{self, Json};
use flea::ui_logic_pl3::host::{self, Host};
use flea::ui_logic_pl3::rows::Reply;
use flea::ui_logic_pl3::view::{Anchor, Keys, Press, Signal, View, CHECK_MS};
use flea::ui_logic_pl3::words::{self, Scope, Snapshot};
use gpui::{Context, FocusHandle, ImageAssetLoader, Resource, Task, Window};
use std::{
    cell::RefCell,
    collections::HashSet,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

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
    /// The next face publishes with its matching complete row window.
    pending_face: Option<crate::list::Typeface>,
    pub focus: FocusHandle,
    pub button: Button,
    pub(super) scrollbar: super::scrollbar::Bar,
    pub menu_open: bool,
    pub width: f32,
    pub(super) height: f32,
    pub presentation_loading: bool,
    pub loading_at: Option<Instant>,
    rows_request: u64,
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
    monitor: Option<Task<()>>,
    decoded: HashSet<Resource>,
    pub(super) retired: Vec<Resource>,
    epoch: Instant,
    /// Starts the Trash watch when Trash opens; None leaves it to a host that already watches.
    pub monitor_start: Option<fn() -> Started>,
    // The last pointer point over the rows, and whether the keyboard has quieted its wash since (#322).
    pub(super) pointer: Option<gpui::Point<gpui::Pixels>>,
    pub(super) quiet: bool,
    /// The strip's controls as the last frame laid them out, for the IPC centre readers.
    pub controls: super::controls::Controls,
}

impl TrashPane {
    pub fn new(
        catalog: Arc<I18n>,
        outside: Box<dyn Outside>,
        ink: Ink,
        preset: &str,
        home: String,
        cx: &mut Context<Self>,
    ) -> Self {
        let typeface = crate::list::Typeface::omarchy_default();
        let board = Board::at(&typeface);
        Self {
            host: Host::new(View::new(board.row_h)),
            catalog,
            outside,
            paint: Rc::new(RefCell::new(ListPaint::new(typeface.font, board, ink))),
            pending_face: None,
            focus: cx.focus_handle(),
            button: Button::default(),
            controls: Default::default(),
            scrollbar: Default::default(),
            menu_open: false,
            width: 0.,
            height: 0.,
            presentation_loading: false,
            loading_at: None,
            rows_request: 0,
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
            pointer: None,
            quiet: false,
        }
    }

    /// The measures at the text size the pane last drew at.
    pub fn board(&self) -> Board {
        self.paint.borrow().board
    }

    fn requested_board(&self) -> Board {
        self.pending_face
            .as_ref()
            .map_or_else(|| self.board(), Board::at)
    }

    /// The window's face: the rows, the strip and the header follow its text size as the list does.
    pub fn set_face(&mut self, face: &crate::list::Typeface, cx: &mut Context<Self>) {
        let board = Board::at(face);
        if self.pending_face.as_ref() == Some(face) {
            return;
        }
        if self.pending_face.is_none() {
            let paint = self.paint.borrow();
            if paint.font == face.font && paint.board == board {
                return;
            }
        }
        let old = self.host.view.row_height;
        self.pending_face = Some(face.clone());
        // Preserve the top row and the full pane height while requesting the newly visible rows.
        let y = self.host.view.scroll_y * board.row_h / old;
        let h = (self.height - board.top()).max(0.);
        self.host.view.row_height = board.row_h;
        self.host.view.set_viewport(y, h);
        self.pump(cx);
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

    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.host.close();
        self.pump(cx);
    }
    pub fn resize(&mut self, width: f32, height: f32, cx: &mut Context<Self>) {
        self.width = width;
        self.height = height;
        let rows = (height - self.requested_board().top()).max(0.);
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
            Heard::Failed(key) => self.outside.monitor_failed(self.catalog.tr(key)),
            Heard::Changed => {
                self.host.view.source_changed();
                self.pump(cx);
            }
        }
        cx.notify();
    }

    /// The strip's back button: TrashView's root.close(), live through a review as the oracle's is.
    pub fn back_pressed(&mut self, cx: &mut Context<Self>) {
        if self.host.view.opened {
            self.host.view.close();
        }
        self.pump(cx);
    }

    pub fn empty_pressed(&mut self, cx: &mut Context<Self>) {
        if self.empty_available() {
            self.host.action("emptyTrash");
        }
        self.pump(cx);
    }

    pub fn click_row(&mut self, index: usize, toggle: bool, cx: &mut Context<Self>) {
        if self.interaction_held() {
            return;
        }
        self.outside.focus();
        let extend = if toggle {
            flea::ui_logic_pl3::selection::Extend::Toggle
        } else {
            flea::ui_logic_pl3::selection::Extend::No
        };
        self.host.view.strip_focus = false;
        self.host.view.choose(index as isize, extend);
        self.pump(cx);
    }
    pub fn right_click(&mut self, index: Option<usize>, x: f32, y: f32, cx: &mut Context<Self>) {
        if self.interaction_held() {
            return;
        }
        self.outside.focus();
        let view = &mut self.host.view;
        if let Some(index) = index {
            let marked = view
                .row_at(index)
                .is_some_and(|row| view.is_selected(&row.uri));
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
        if self.interaction_held() {
            return;
        }
        let (y, h) = (self.host.view.scroll_y + dy, self.host.view.height);
        self.host.view.set_viewport(y, h);
        self.pump(cx);
    }

    fn open_menu(&mut self, at: Anchor, selection: bool) {
        let view = &self.host.view;
        let items = if selection {
            host::selection_menu(view.busy(), &self.hidden)
        } else {
            host::trash_menu(view.total, view.busy())
        };
        let board = self.board();
        let at = match at {
            Anchor::Row(index) => At {
                x: board.pad_x,
                y: board.top() + (index + 1) as f32 * board.row_h - view.scroll_y + board.menu_drop,
            },
            Anchor::Top => At {
                x: self.width / 2.,
                y: board.strip_h,
            },
            Anchor::Point(x, y) => At { x, y },
        };
        self.menu_open = true;
        self.outside.menu(items, at);
    }
    /// The menu primitive closed, with the picked row's action or none.
    pub fn menu_action(&mut self, action: Option<&str>, cx: &mut Context<Self>) {
        self.menu_open = false;
        if self.interaction_held() {
            return self.pump(cx);
        }
        match action {
            Some("open") => self.host.open(""),
            Some(action) => self.host.action(action),
            None => {}
        }
        self.pump(cx);
    }
    /// op1 answered the card: the destructive button's token, or None for Cancel and Escape.
    pub fn card_answered(&mut self, token: Option<u64>, cx: &mut Context<Self>) {
        let Some(shown) = self.card.as_ref().map(|card| card.token) else {
            return;
        };
        if token.is_some_and(|token| token != shown) {
            return;
        }
        self.card = None;
        self.host.view.confirmed(token);
        self.pump(cx);
    }

    /// Only the host's tagged, still-displayed card can cancel while its replacement is pending.
    pub(crate) fn cancel_held_card(&mut self, cx: &mut Context<Self>) {
        if self.card.is_none() && self.host.view.confirming {
            self.host.view.confirmed(None);
            self.pump(cx);
        }
    }

    /// Sends what the session queued, hands every signal to its seam and refreshes the paint inputs.
    pub fn pump(&mut self, cx: &mut Context<Self>) {
        for line in self.host.drain() {
            self.outside.send(line);
        }
        for signal in self.host.view.signals() {
            match signal {
                Signal::Settle {
                    generation,
                    delay_ms,
                } => self.settle(generation, delay_ms, cx),
                Signal::Status { text, error } => self.outside.status(text, error),
                Signal::Arm(text) => self.outside.arm(text),
                Signal::Result {
                    headline,
                    detail,
                    error,
                } => self.outside.result(headline, detail, error),
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
                cx.background_executor()
                    .timer(Duration::from_millis(CHECK_MS))
                    .await;
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
            cx.background_executor()
                .timer(Duration::from_millis(delay_ms))
                .await;
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
        let Some(snapshot) = view.confirmation.clone() else {
            return;
        };
        let scope = if snapshot.all {
            Scope::Trash
        } else {
            Scope::Items
        };
        let card = Card {
            title: words::confirm_title(&self.catalog, &snapshot),
            body: words::confirm_body(&self.catalog, &snapshot, scope),
            confirm: self.catalog.tr(if snapshot.all {
                "trash.confirm.empty_button"
            } else {
                "trash.confirm.delete_button"
            }),
            cancel: self.catalog.tr("trash.confirm.cancel"),
            token: snapshot.token,
            empty: snapshot.all,
        };
        self.card = Some(snapshot);
        self.outside.card(Some(card));
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
