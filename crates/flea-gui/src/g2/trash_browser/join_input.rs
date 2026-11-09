//! Trash owns one raw arm key per press and returns all other native keys through the shipped table.
use crate::{
    state::Shell,
    window::{
        dispatch::Order,
        input::{event_input, ActionEvent},
    },
};
use flea::keys::{self, Context as KeyContext, Match};
use gpui::{Context, KeyDownEvent, Window};

impl Shell {
    /// Resolved Escape, including IPC, uses the same Trash owner as a native key.
    pub(crate) fn trash_escape(
        &mut self,
        action: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if action != "escape" || self.keyboard.context != KeyContext::Listing {
            return false;
        }
        let Some(pane) = self
            .trash
            .as_ref()
            .filter(|host| host.side == self.focus_side && host.pane.read(cx).view().opened)
            .map(|host| host.pane.clone())
        else {
            return false;
        };
        pane.update(cx, |pane, cx| pane.action("escape", false, cx));
        self.flush_trash(window, cx);
        true
    }

    pub(crate) fn trash_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.trash_opened(cx) {
            return false;
        }
        if self
            .trash
            .as_ref()
            .is_some_and(|host| host.side != self.focus_side)
        {
            return false;
        }
        if let Some(overlay) = self
            .overlay
            .clone()
            .filter(|overlay| overlay.read(cx).model.open())
        {
            overlay.update(cx, |overlay, cx| overlay.key(event, window, cx));
            return true;
        }
        let host = self.trash.as_ref().expect("opened");
        let pane = host.pane.clone();
        let menu_open = host.menu.borrow().opened();
        if !menu_open && !pane.read(cx).focus.contains_focused(window, cx) {
            // The window's own handle is the list stop, and on the side Trash covers that list is Trash's.
            if !self.focus.is_focused(window) {
                return false;
            }
            let focus = pane.read(cx).focus.clone();
            window.focus(&focus, cx);
        }
        let preset = self
            .launch
            .settings
            .data
            .get("keys")
            .and_then(flea::jsondoc::Json::as_str)
            .unwrap_or("default");
        let input = event_input(event, window.modifiers());
        let action = if input.modifiers == 0 && input.key.eq_ignore_ascii_case("backspace") {
            Some("parent")
        } else if input.modifiers == 0 && input.key.eq_ignore_ascii_case("f5") {
            Some("reload")
        } else {
            match self.keyboard.lookup(
                preset,
                if menu_open {
                    KeyContext::Menu
                } else {
                    KeyContext::Listing
                },
                event,
                window.modifiers(),
                window.layout_mirrored(),
            ) {
                Match::Action(action) => Some(action),
                _ => None,
            }
        };
        // The window's Settings chord is not the Trash view's to operate, so it reaches the window.
        if !menu_open && action == Some("settings") {
            return false;
        }
        if let Some(action) = action {
            if menu_open {
                self.trash_menu_action(action, window, cx);
            } else {
                pane.update(cx, |pane, cx| pane.action(action, event.is_held, cx));
            }
        } else if !menu_open {
            pane.update(cx, |pane, cx| pane.action("", event.is_held, cx));
        }
        self.flush_trash(window, cx);
        true
    }

    pub(super) fn forward_trash_action(
        &mut self,
        action: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if action == "quit" {
            self.close(window, cx);
            return;
        }
        if action == "windowNew" {
            self.new_window();
            return;
        }
        if self.settings_action(action, window, cx) {
            return;
        }
        if action == "sidebar" {
            self.toggle_sidebar(window, cx);
            return;
        }
        if action == "pathBar" {
            self.chrome_action("pathBar", window, cx);
            return;
        }
        let Some(action) = keys::PRESET_KEYS
            .iter()
            .chain(keys::SHARED_KEYS)
            .find(|candidate| candidate.action == action)
        else {
            return;
        };
        let event = ActionEvent {
            action: action.action,
            context: KeyContext::Listing,
            seat: Default::default(),
            pair_armed: false,
        };
        self.dispatch(Order::Action(event), window, cx);
    }
}
