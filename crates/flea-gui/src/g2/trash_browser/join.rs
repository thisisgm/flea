//! The live window's Trash join; the existing pane, backend worker, menu and operation card retain ownership.
use super::{
    ink::Ink,
    pane::TrashPane,
    seams::{At, Card, Outside},
};
use crate::{list::Palette, palette::Theme, state::Shell, window::composition::Geometry};
use flea::ui_logic_trash_browser::host::Item;
use gpui::{AppContext, Context, Entity, Subscription, Window};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

pub(super) enum Effect {
    Focus,
    Send(String),
    Card(Option<Card>),
    Menu(Vec<Item>, At),
    /// A sentence for the one-line bar, the detail for the strip under it, and whether it is an error.
    Status(String, String, bool),
    MonitorFailed(String),
    Arm(String),
    Dismiss,
    Count(usize),
    Leave(bool),
    Back,
    Action(String),
    Swept(i64),
}

struct OutsideHost(Rc<RefCell<Vec<Effect>>>, Rc<Cell<bool>>);
impl Outside for OutsideHost {
    fn focus(&mut self) {
        self.0.borrow_mut().push(Effect::Focus);
    }
    fn send(&mut self, line: String) {
        self.0.borrow_mut().push(Effect::Send(line));
    }
    fn card(&mut self, card: Option<Card>) {
        self.0.borrow_mut().push(Effect::Card(card));
    }
    fn menu(&mut self, items: Vec<Item>, at: At) {
        self.0.borrow_mut().push(Effect::Menu(items, at));
    }
    fn status(&mut self, text: String, error: bool) {
        self.0
            .borrow_mut()
            .push(Effect::Status(text, String::new(), error));
    }
    fn arm(&mut self, text: String) {
        self.0.borrow_mut().push(Effect::Arm(text));
    }
    fn result(&mut self, headline: String, detail: String, error: bool) {
        // TrashHost.qml's operationResult: the headline stays on the bar, each failure goes to the detail strip under it.
        self.0
            .borrow_mut()
            .push(Effect::Status(headline, detail, error));
    }
    fn escape_taken(&mut self) -> bool {
        if !self.1.replace(false) {
            return false;
        }
        self.0.borrow_mut().push(Effect::Dismiss);
        true
    }
    fn count(&mut self, total: usize) {
        self.0.borrow_mut().push(Effect::Count(total));
    }
    fn monitor_failed(&mut self, text: String) {
        self.0.borrow_mut().push(Effect::MonitorFailed(text));
    }
    fn leave(&mut self, forward: bool) {
        self.0.borrow_mut().push(Effect::Leave(forward));
    }
    fn back(&mut self) {
        self.0.borrow_mut().push(Effect::Back);
    }
    fn action(&mut self, name: String) {
        self.0.borrow_mut().push(Effect::Action(name));
    }
    fn swept(&mut self, day: i64) {
        self.0.borrow_mut().push(Effect::Swept(day));
    }
}

/// Rail owners use this event for the existing count slot.
#[derive(Clone, Copy, Debug)]
pub enum TrashEvent {
    Count(usize),
}
impl gpui::EventEmitter<TrashEvent> for Shell {}

pub struct TrashHost {
    pub pane: Entity<TrashPane>,
    pub count: usize,
    pub drawn_message: gpui::SharedString,
    pending_count: Option<usize>,
    pub side: usize,
    pub backend_side: usize,
    pub presentation: super::presentation::Presentation,
    pub(super) effects: Rc<RefCell<Vec<Effect>>>,
    pub(super) menu: Rc<RefCell<crate::g2::context_menus::menu::Menu>>,
    pub(super) menu_selection: flea::ui_logic_trash_browser::selection::Selection,
    pub(super) card_token: Option<u64>,
    pub(super) overlay_watch: Option<Subscription>,
    pub messages: Entity<crate::g2::status_feedback::line::MessageLine>,
    pub(super) dismissible: Rc<Cell<bool>>,
    pub(super) message_watches: Vec<Subscription>,
    _watch: Subscription,
    _focus_out: Subscription,
    pub(super) sweep_timer: Option<gpui::Task<()>>,
}

impl Shell {
    pub(crate) fn mount_trash(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.trash.is_some() {
            return;
        }
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        let ink = Ink::interim(
            &Palette::inherited(&window.text_style(), theme),
            gpui::px(self.corner),
        );
        let effects = Rc::new(RefCell::new(Vec::new()));
        let dismissible = Rc::new(Cell::new(false));
        let catalog = self.locale.catalog.clone();
        let preset = self
            .launch
            .settings
            .data
            .get("keys")
            .and_then(flea::jsondoc::Json::as_str)
            .unwrap_or("default");
        let home = std::env::var("HOME").unwrap_or_default();
        let messages = cx.new(|cx| {
            crate::g2::status_feedback::line::MessageLine::new(preset, catalog.clone(), cx)
        });
        let pane = cx.new(|cx| {
            TrashPane::new(
                catalog,
                Box::new(OutsideHost(effects.clone(), dismissible.clone())),
                ink,
                preset,
                home,
                cx,
            )
        });
        if let Some(hidden) = self
            .launch
            .settings
            .data
            .get("menu")
            .and_then(|m| m.get("hidden"))
            .and_then(flea::jsondoc::Json::as_array)
        {
            pane.update(cx, |pane, _| {
                pane.hidden = hidden
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            });
        }
        let watch = cx.observe_in(&pane, window, |shell, _, window, cx| {
            shell.flush_trash(window, cx)
        });
        let focus = pane.read(cx).focus.clone();
        let focused_pane = pane.clone();
        let focus_out = cx.on_focus_out(&focus, window, move |shell, _, window, cx| {
            focused_pane.update(cx, |pane, cx| {
                pane.host.view.arm.clear();
                pane.pump(cx);
            });
            shell.flush_trash(window, cx);
        });
        self.trash = Some(TrashHost {
            pane,
            count: 0,
            drawn_message: Default::default(),
            pending_count: None,
            side: self.focus_side,
            backend_side: self.focus_side,
            presentation: super::presentation::Presentation::new(cx.background_executor().now()),
            effects,
            menu: Rc::new(RefCell::new(Default::default())),
            menu_selection: Default::default(),
            card_token: None,
            overlay_watch: None,
            _watch: watch,
            _focus_out: focus_out,
            messages,
            dismissible,
            message_watches: Vec::new(),
            sweep_timer: None,
        });
        self.watch_trash_messages(window, cx);
        self.schedule_trash_sweep(window, cx);
    }
    /// Open Trash or deliver an action to its pane.
    pub fn trash_action(
        &mut self,
        action: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match action {
            "openTrash"
            | "emptyTrash"
            | "restoreAll"
            | "restoreTrashSelection"
            | "deleteTrashSelection" => {}
            _ => return false,
        }
        self.mount_trash(window, cx);
        self.clear_under_trash(cx);
        let geometry = self.geometry(window);
        let host = self.trash.as_mut().expect("mounted Trash");
        host.sweep_timer = None;
        let view = host.pane.read(cx).view();
        if !view.opened && view.confirming {
            return true;
        }
        let opening = !view.opened && !view.operation_active;
        if !view.opened {
            host.side = self.focus_side;
            if opening {
                host.backend_side = self.focus_side;
            }
        }
        let pane = host.pane.clone();
        let rect = geometry.panes[host.side];
        let height = if self.view == crate::window::startup::View::Dual {
            rect.height
        } else {
            geometry.status.y
        };
        if opening {
            self.begin_trash_presentation(window, cx);
        }
        let face = self.active_typeface();
        pane.update(cx, |pane, cx| {
            pane.set_face(&face, cx);
            pane.resize(rect.width, height, cx);
            if matches!(action, "openTrash" | "emptyTrash" | "restoreAll") {
                pane.open(if action == "openTrash" { "" } else { action }, cx);
            } else {
                pane.menu_action(
                    Some(if action == "deleteTrashSelection" {
                        "deletePermanently"
                    } else {
                        action
                    }),
                    cx,
                );
            }
        });
        let focus = pane.read(cx).focus.clone();
        window.focus(&focus, cx);
        self.flush_trash(window, cx);
        true
    }
    /// Tabs.js dropOverlay: a tab opened or switched on Trash's side closes it, and the keyboard returns to the window.
    pub(crate) fn drop_trash(&mut self, side: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(pane) = self
            .trash
            .as_ref()
            .filter(|host| host.side == side && host.pane.read(cx).view().opened)
            .map(|host| host.pane.clone())
        else {
            return;
        };
        pane.update(cx, |pane, cx| pane.close(cx));
        self.flush_trash(window, cx);
    }

    pub fn trash_count(&self) -> usize {
        self.trash.as_ref().map_or(0, |host| host.count)
    }
    pub(crate) fn trash_opened(&self, cx: &gpui::App) -> bool {
        self.trash
            .as_ref()
            .is_some_and(|host| host.pane.read(cx).view().opened)
    }

    /// The focus an overlay hands back: the covering Trash pane while it holds the focused side, else the window.
    pub(crate) fn overlay_return_focus(&self, cx: &gpui::App) -> gpui::FocusHandle {
        match &self.trash {
            Some(host) if host.side == self.focus_side && host.pane.read(cx).view().opened => {
                host.pane.read(cx).focus.clone()
            }
            _ => self.focus.clone(),
        }
    }

    pub(crate) fn size_trash(&mut self, geometry: &Geometry, cx: &mut Context<Self>) {
        let Some(host) = &self.trash else {
            return;
        };
        if !host.pane.read(cx).view().opened {
            return;
        }
        let rect = geometry.panes[host.side];
        let dual = self.view == crate::window::startup::View::Dual;
        let height = if dual { rect.height } else { geometry.status.y };
        // The rows' own width: a strip drawn from the window's left edge keeps them beside the places.
        let inset = if dual { 0. } else { geometry.cover_inset };
        let rows_width = (rect.width - inset).max(0.);
        let unified =
            flea::ui_logic_settings_pages::pages::word(&self.launch.settings.data, &["windowBar"], "mixed")
                != "mixed";
        let face = self.active_typeface();
        let board = super::board::Board::at(&face);
        let pane = host.pane.read(cx);
        if pane.width != rows_width
            || pane.inset != inset
            || pane.unified != unified
            || pane.height != height
            || pane.board() != board
            || pane.view().height != (height - board.top()).max(0.)
        {
            host.pane.update(cx, |pane, cx| {
                pane.set_face(&face, cx);
                pane.inset = inset;
                pane.unified = unified;
                pane.paint.borrow_mut().unified = unified;
                cx.notify();
                pane.resize(rows_width, height, cx);
            });
        }
    }

    pub(crate) fn trash_line(&mut self, side: usize, line: &str, cx: &mut Context<Self>) {
        if let Some(host) = self.trash.as_ref().filter(|host| host.backend_side == side) {
            host.pane.update(cx, |pane, cx| pane.line(line, cx));
        }
    }

    pub(super) fn flush_trash(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(host) = &self.trash else {
            return;
        };
        let (pane, side) = (host.pane.clone(), host.side);
        let backend_side = host.backend_side;
        let effects = std::mem::take(&mut *host.effects.borrow_mut());
        for effect in effects {
            match effect {
                Effect::Focus => self.focus_side = side,
                Effect::Send(line) => {
                    let result = self.panes[backend_side]
                        .as_ref()
                        .and_then(|host| host.backend.as_ref())
                        .map(|backend| backend.trash_line(line));
                    if !matches!(result, Some(Ok(()))) {
                        pane.update(cx, |pane, cx| pane.transport_failed(cx));
                    }
                }
                Effect::Card(card) => self.trash_card(card, window, cx),
                Effect::Menu(items, at) => self.open_trash_menu(items, at, window, cx),
                Effect::Status(text, detail, error) => {
                    self.trash_say_detail(&text, &detail, error, cx)
                }
                Effect::MonitorFailed(text) => self.trash_monitor_failed(&text, cx),
                Effect::Arm(text) => self.trash_arm(&text, cx),
                Effect::Dismiss => self.trash_dismiss(cx),
                Effect::Count(total) => {
                    self.trash.as_mut().expect("mounted").pending_count = Some(total);
                }
                Effect::Back => {
                    window.focus(&self.focus, cx);
                }
                Effect::Leave(forward) => self.trash_leave(forward, window, cx),
                Effect::Action(action) => {
                    if !self.trash_action(&action, window, cx) {
                        self.forward_trash_action(&action, window, cx);
                    }
                }
                Effect::Swept(day) => self.record_trash_sweep(day, window, cx),
            }
        }
        if self
            .trash
            .as_ref()
            .is_some_and(|host| host.card_token.is_some())
            && !pane.read(cx).view().confirming
        {
            self.trash_card(None, window, cx);
        }
        let opened = pane.read(cx).view().opened;
        self.sync_trash_presentation(window, cx);
        let host = self.trash.as_mut().expect("mounted");
        if !pane.read(cx).interaction_held()
            && host.presentation.stage != super::presentation::Stage::Holding
        {
            if let Some(total) = host.pending_count.take() {
                host.count = total;
                cx.emit(TrashEvent::Count(total));
            }
        }
        if !opened {
            self.trash
                .as_ref()
                .expect("mounted")
                .menu
                .borrow_mut()
                .close();
        }
        cx.notify();
    }
}
