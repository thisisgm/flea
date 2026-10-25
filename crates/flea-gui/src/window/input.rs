use super::{dispatch::Order, lifecycle::Phase, startup};
use crate::{pane::requests, state::Shell};
use gpui::{px, Context, KeyDownEvent, ScrollWheelEvent, Window};
#[path = "keyboard.rs"]
mod keyboard;
pub use keyboard::*;

#[derive(Clone, Copy, Debug)]
pub struct ActionEvent {
    pub action: &'static str,
    pub context: flea::keys::Context,
    pub seat: flea::keys::sequence::Seat,
    pub pair_armed: bool,
}

impl gpui::EventEmitter<ActionEvent> for Shell {}

impl ActionEvent {
    /// An action with no seat and no armed pair, as a menu or a modal key resolves it.
    pub(crate) fn bare(action: &'static str, context: flea::keys::Context) -> Self {
        Self {
            action,
            context,
            seat: Default::default(),
            pair_armed: false,
        }
    }
}

impl Shell {
    pub fn new_window(&self) {
        let Some(host) = &self.panes[self.focus_side] else {
            return;
        };
        let mut command = startup::new_window_command(&host.path);
        let locale = self.locale.clone();
        let _ = std::thread::Builder::new()
            .name("flea-window".into())
            .spawn(move || {
                if let Err(error) = command.spawn().and_then(|mut child| child.wait()) {
                    eprintln!(
                        "flea-gui: {}",
                        locale
                            .catalog
                            .tr_args("window.open_failed", &[&error.to_string()])
                    );
                }
            });
    }

    pub(crate) fn key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.lifecycle.phase() != Phase::Open {
            return;
        }
        // Escape ends this window's own drag before any card or listing reads the key.
        if event.keystroke.key == "escape" && self.escape_file_drag(window, cx) {
            cx.stop_propagation();
            return;
        }
        // Every modal key belongs to the card; a cardless Escape still reaches the window.
        if let Some(overlay) = self.overlay.clone() {
            let modal = overlay.read_with(cx, |card, cx| card.modal(cx));
            if modal
                || (event.keystroke.key == "escape" && modifiers(event.keystroke.modifiers) == 0)
            {
                let showing = overlay.read_with(cx, |card, cx| card.showing(cx));
                overlay.update(cx, |card, cx| card.key(event, window, cx));
                let still = overlay.read_with(cx, |card, cx| card.showing(cx));
                if !still {
                    self.overlay = None;
                }
                cx.notify();
                if showing {
                    cx.stop_propagation();
                    return;
                }
            }
        }
        if self.archive_key(event, window, cx) {
            cx.stop_propagation();
            return;
        }
        if event.keystroke.key == "escape" && self.dual_drag.is_some() {
            self.dual_cancel(cx);
            cx.stop_propagation();
            return;
        }
        if event.keystroke.modifiers.control && event.keystroke.key == "q" {
            self.close(window, cx);
            cx.stop_propagation();
            return;
        }
        if self.settings_dismiss(event, cx) {
            return;
        }
        if self.trash_key(event, window, cx) {
            cx.stop_propagation();
            return;
        }
        if self.network_key(event, window, cx) {
            cx.stop_propagation();
            return;
        }
        if self.menu_key(event, window, cx) {
            return;
        }
        if self.path_holds_keyboard(window, cx) {
            return;
        }
        if event.keystroke.key == "escape"
            && modifiers(event.keystroke.modifiers) == 0
            && self.keyboard.context == flea::keys::Context::Listing
            && self.status_escape(cx)
        {
            cx.stop_propagation();
            return;
        }
        if !self.input_blocked(cx) && self.filter_key(event, window, cx) {
            cx.stop_propagation();
            return;
        }
        if !self.input_blocked(cx) && self.search_key(event, window, cx) {
            cx.stop_propagation();
            return;
        }
        if self.input_blocked(cx) {
            cx.stop_propagation();
            return;
        }
        // Ctrl+P is the listing's Print; the keymap leaves it unbound, so it is read here like Ctrl+Q.
        if self.keyboard.context == flea::keys::Context::Listing
            && event.keystroke.key == "p"
            && event.keystroke.modifiers.control
            && !event.keystroke.modifiers.shift
            && !event.keystroke.modifiers.alt
            && !event.keystroke.modifiers.platform
            && self.output_print_key(window, cx)
        {
            cx.stop_propagation();
            return;
        }
        if let Some(stop) = self.rail_key(event, window, cx) {
            if stop {
                cx.stop_propagation();
            }
            return;
        }
        if modifiers(event.keystroke.modifiers) == 0
            && (self.view != startup::View::Dual || self.focus.is_focused(window))
        {
            if let Some(host) = &mut self.panes[self.focus_side] {
                // The seam acts only while idle, so no plain key touches a listing being replaced.
                if !host.pane.busy() && self.seam.key(&event.keystroke.key, &mut host.pane) {
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
            }
        }
        let Some(host) = &self.panes[self.focus_side] else {
            return;
        };
        let seat = flea::keys::sequence::Seat {
            listing: host.pane.held.listing as u64,
            cursor: host.pane.cursor,
            view: self.view as u64,
            focus: self.focus_side as u64,
            ..Default::default()
        };
        let name = self
            .launch
            .settings
            .data
            .get("keys")
            .and_then(flea::jsondoc::Json::as_str)
            .unwrap_or("default");
        let pair_armed = self.keyboard.pair_armed(name, seat);
        let resolved = self.keyboard.resolve(name, event, window.modifiers(), seat);
        let dispatch = if resolved == Dispatch::Composition {
            resolved
        } else {
            self.grid_sideways(event)
                .map(Dispatch::Action)
                .unwrap_or(resolved)
        };
        let action = match dispatch {
            Dispatch::Unbound | Dispatch::Composition => return,
            Dispatch::Suppressed | Dispatch::Armed => {
                cx.stop_propagation();
                return;
            }
            Dispatch::Action(action) => action,
        };
        let busy = host.pane.busy();
        if matches!(action, "focusNext" | "focusPrevious")
            && self.dual_tab(action == "focusNext", window, cx)
        {
            cx.stop_propagation();
            return;
        }
        // A listing out swallows every row action, modified or not, but the answered list.
        if matches!(action, "focusNext" | "focusPrevious") {
            self.input_action(action, window, cx);
            cx.stop_propagation();
            return;
        }
        if self.keyboard.context == flea::keys::Context::Listing && requests::swallows(busy, action)
        {
            cx.stop_propagation();
            return;
        }
        // A standing filter takes its keys ahead of the key-only handlers; the listing family runs them.
        if !self.filter_claims(action, cx) && self.key_only_action(action, window, cx) {
            cx.stop_propagation();
            return;
        }
        let context = self.keyboard.context;
        let event = ActionEvent {
            action,
            seat,
            context,
            pair_armed,
        };
        self.dispatch(Order::Action(event), window, cx);
        cx.stop_propagation();
    }

    /// The key-only handlers that run before the dispatch point; true when one took the action.
    fn key_only_action(
        &mut self,
        action: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.search_action(action, window, cx)
            || self.trash_action(action, window, cx)
            || self.settings_action(action, window, cx)
            || self.grid_key(action, cx)
            || ((action == "escape"
                || self.view != startup::View::Dual
                || self.focus.is_focused(window))
                && self.input_action(action, window, cx))
            || self.operation_action(action, cx)
            || self.clipboard_action(action, window, cx)
        {
            return true;
        }
        if action == "menu" {
            self.menu_keyboard_open(window, cx);
        } else if action == "autofitColumns" {
            self.fit_list_columns(window, cx);
        } else if action == "pathBar" {
            self.chrome_action(action, window, cx);
        } else if action == "windowNew" {
            self.new_window();
        } else if action == flea::ui_logic_mn2::agent::ID {
            self.agent_key(cx);
        } else if let Some(view) = match action {
            "viewList" => Some(startup::View::List),
            "viewColumns" => Some(startup::View::Columns),
            "viewGrid" => Some(startup::View::Grid),
            "viewDual" => Some(startup::View::Dual),
            _ => None,
        } {
            self.set_view(view, window, cx);
        } else {
            return false;
        }
        true
    }

    pub(crate) fn wheel(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.menu_wheel(event, window, cx) {
            return;
        }
        if self.input_blocked(cx) || self.view != startup::View::Grid {
            return;
        }
        let Some(host) = self.panes[self.focus_side].as_ref() else {
            return;
        };
        // The root hears chrome and status wheels too; only the painted Grid owns scrolling.
        if host.drawn_view != Some(startup::View::Grid)
            || host.grid.as_ref().is_none_or(|grid| {
                let drawn = grid.state.borrow();
                drawn.layout.is_none() || !drawn.bounds.contains(&event.position)
            })
        {
            return;
        }
        let row = host.pane.row_height();
        self.grid_scroll(-f32::from(event.delta.pixel_delta(px(row)).y), cx);
    }
}
