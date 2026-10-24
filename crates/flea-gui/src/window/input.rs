use super::{lifecycle::Phase, startup};
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
        if self.search_action(action, window, cx) {
            cx.stop_propagation();
            return;
        }
        if self.trash_action(action, window, cx) {
            cx.stop_propagation();
            return;
        }
        if self.settings_action(action, window, cx) {
            cx.stop_propagation();
            return;
        }
        if self.grid_key(action, cx) {
            cx.stop_propagation();
            return;
        }
        if (action == "escape" || self.view != startup::View::Dual || self.focus.is_focused(window))
            && self.input_action(action, window, cx)
        {
            cx.stop_propagation();
            return;
        }
        if self.operation_action(action, cx) {
            cx.stop_propagation();
            return;
        }
        if action == "menu" {
            self.menu_keyboard_open(window, cx);
        } else if action == "autofitColumns" {
            self.fit_list_columns(window, cx);
        } else if action == "pathBar" {
            self.chrome_action(action, window, cx);
        } else if action == "windowNew" {
            self.new_window();
        } else if let Some(view) = match action {
            "viewList" => Some(startup::View::List),
            "viewColumns" => Some(startup::View::Columns),
            "viewGrid" => Some(startup::View::Grid),
            "viewDual" => Some(startup::View::Dual),
            _ => None,
        } {
            self.set_view(view, window, cx);
        } else {
            cx.emit(ActionEvent {
                action,
                seat,
                context: self.keyboard.context,
                pair_armed,
            });
        }
        cx.stop_propagation();
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
