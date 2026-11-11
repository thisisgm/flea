use super::{lifecycle::Phase, startup};
use crate::state::Shell;
use gpui::{Context, KeyDownEvent, Window};
#[path = "keyboard.rs"]
mod keyboard;
#[path = "input_run.rs"]
mod run;
#[path = "input_wheel.rs"]
mod wheel;
pub use keyboard::*;
pub(crate) use run::KeyRun;

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
        // Any later key takes the keyboard back from an Enter still waiting on its rows.
        for host in self.panes.iter_mut().flatten() {
            host.filter.enter_after = None;
            host.search.enter_after = None;
        }
        if self.lifecycle.phase() != Phase::Open || self.band_key(event, window, cx) {
            return;
        }
        self.refresh_user_keys_layout(window, cx);
        self.sync_text_input(window, cx);
        // Escape ends this window's own drag before any card or listing reads the key.
        if event.keystroke.key == "escape"
            && (self.escape_file_drag(window, cx) || self.escape_tab_drag(window, cx))
        {
            cx.stop_propagation();
            return;
        }
        // The unlock and share cards take typed text through the platform, so their text keys keep propagating.
        if self.card_takes_text(event, window, cx) {
            return;
        }
        if let Some(view) = self
            .permissions
            .view
            .clone()
            .filter(|view| view.read(cx).opened)
        {
            view.update(cx, |view, cx| view.key(event, window, cx));
            cx.stop_propagation();
            return;
        }
        // Every modal key belongs to the card; a cardless Escape still reaches the window.
        if let Some(overlay) = self.overlay.clone() {
            let modal = overlay.read_with(cx, |card, cx| card.modal(cx));
            let menu = self.menu_opened() || self.menu.presentation.at.is_some();
            if modal
                || (!menu
                    && event.keystroke.key == "escape"
                    && modifiers(event.keystroke.modifiers) == 0)
            {
                let taken = overlay.update(cx, |card, cx| card.dispatch_key(event, window, cx));
                let still = overlay.read_with(cx, |card, cx| card.showing(cx));
                if !still {
                    self.overlay = None;
                }
                cx.notify();
                if taken {
                    cx.stop_propagation();
                    return;
                }
            }
        }
        // The Properties card holds every key while it shows.
        if self.properties_key(event, window, cx) {
            return;
        }
        if self.plugin_input_key(event, window, cx) {
            return;
        }
        // The run card holds every key while it shows.
        if self.run_card_key(event, window, cx) {
            cx.stop_propagation();
            return;
        }
        // The Open With card holds every key while it shows; its search's typed text keeps propagating.
        if self.open_with_key(event, window, cx) {
            return;
        }
        // The rename editor and the bulk rename card hold every key while open.
        if self.rename_key(event, window, cx) {
            self.rename_sync_gate(window, cx);
            return;
        }
        if self.archive_key(event, window, cx) {
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
        if self.menu_key(event, window, cx) {
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
        if self.path_holds_keyboard(window, cx) {
            return;
        }
        // An open Quick Look card takes every key through the dispatch point.
        if self.quick_look_key(event, window, cx) {
            cx.stop_propagation();
            return;
        }
        // Only the composing line reads a key under marked text, even the platform's insert; a dead key carries no owner.
        if let Some(owner) = self.keyboard.text_owner() {
            match owner {
                crate::window::text_input::Owner::Filter(_) => {
                    self.filter_key(event, window, cx);
                }
                crate::window::text_input::Owner::Find(_) => {
                    self.search_key(event, window, cx);
                }
                crate::window::text_input::Owner::Name => {}
            }
            self.keyboard.disarm();
            cx.stop_propagation();
            return;
        }
        // Modal cards, menus and fields have already had first refusal on Escape.
        if event.keystroke.key == "escape"
            && modifiers(event.keystroke.modifiers) == 0
            && self.carry_escape(window, cx)
        {
            cx.stop_propagation();
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
        if !self.wheel_blocked(cx)
            && (self.chrome_key(event, window, cx) || self.filter_key(event, window, cx))
        {
            cx.stop_propagation();
            return;
        }
        if !self.wheel_blocked(cx) && self.search_key(event, window, cx) {
            cx.stop_propagation();
            return;
        }
        if self.wheel_blocked(cx) {
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
        // Focus.js keyContext reads the live preview on every key, so a frame drawn after the focus moved still counts.
        let column_focused = if let Some(preview) = host
            .preview
            .as_ref()
            .filter(|p| p.read(cx).focus.is_focused(window))
        {
            self.keyboard.context = preview.read(cx).key_context();
            true
        } else {
            false
        };
        let pair_armed = self.keyboard.pair_armed(name, seat);
        let arm_live = self.escape_arm_live(seat);
        let resolved = self.keyboard.resolve(
            name,
            event,
            window.modifiers(),
            window.layout_mirrored(),
            seat,
        );
        let dispatch = if resolved == Dispatch::Composition {
            resolved
        } else {
            self.grid_sideways(event, window.layout_mirrored())
                .map(Dispatch::Action)
                .unwrap_or(resolved)
        };
        let listing = self.keyboard.context == flea::keys::Context::Listing;
        self.operations
            .fileops
            .end_for_listing_key(listing, &dispatch);
        let action = match dispatch {
            Dispatch::Unbound => {
                let bound = if listing {
                    self.user_key(name, event, window.modifiers())
                } else {
                    None
                };
                match bound {
                    Some(crate::window::user_keys::Bound::Builtin(action)) => action,
                    Some(crate::window::user_keys::Bound::Plugin(id)) => {
                        self.run_user_plugin_key(&id, event, window, cx);
                        return;
                    }
                    None => {
                        self.unbound_listing_key(event, cx);
                        return;
                    }
                }
            }
            Dispatch::Composition => return,
            Dispatch::Suppressed | Dispatch::Armed => {
                if dispatch == Dispatch::Armed {
                    self.keyboard.capture_sequence_path(host.pane.path());
                }
                cx.stop_propagation();
                return;
            }
            Dispatch::Action(action) => action,
        };
        // A focused column preview keeps Tab and Shift+Tab on its own surface.
        if column_focused && matches!(action, "focusNext" | "focusPrevious") {
            cx.stop_propagation();
            return;
        }
        if self.run_key_action(action, seat, pair_armed, arm_live, window, cx) != KeyRun::Kept {
            cx.stop_propagation();
        }
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
            || self.trash_escape(action, window, cx)
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
            self.cursor_menu_key(window, cx);
        } else if action == "autofitColumns" {
            self.fit_list_columns(window, cx);
        } else if action == "pathBar" {
            self.chrome_action(action, window, cx);
        } else if action == "windowNew" {
            self.new_window();
        } else if action == "eject" {
            self.release_device(false, cx);
        } else if action == "print" {
            // keys.toml's ctrl-p row (the menu's Print… hint); Print claims a refusal as well as an opened card.
            self.output_print_key(window, cx);
        } else if action == flea::ui_logic_custom_actions::agent::ID {
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
}
