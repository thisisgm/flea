//! Keyboard focus within Trash stays inside its own strip and listing.
use super::*;

impl TrashPane {
    /// A key action from the generated table, while Trash holds the keyboard.
    pub fn action(&mut self, name: &str, repeat: bool, cx: &mut Context<Self>) {
        if self.interaction_held()
            && !matches!(name, "escape" | "parent" | "back" | "quit" | "windowNew")
        {
            return;
        }
        if name == "focusPrevious" || (name == "focusNext" && self.host.view.strip_focus) {
            self.host.view.arm.clear();
            return self.focus_strip(!self.host.view.strip_focus, cx);
        }
        if self.host.view.strip_focus && matches!(name, "open" | "activate") {
            if self.empty_available() {
                self.host.view.prepare(true);
            }
            return self.pump(cx);
        }
        if name == "escape" && self.host.view.confirmation.is_none() && self.outside.escape_taken()
        {
            self.host.view.arm.clear();
            return self.pump(cx);
        }
        if matches!(
            name,
            "cursorDown" | "cursorUp" | "cursorFirst" | "cursorLast" | "pageDown" | "pageUp"
        ) {
            self.host.view.strip_focus = false;
        }
        let (now, keys) = (
            self.now(),
            Keys {
                arm: &self.keys.0,
                alone: &self.keys.1,
            },
        );
        let press = match name {
            "parent" | "back" => Press::Backspace,
            "reload" => Press::F5,
            other => Press::Action(other),
        };
        self.host
            .view
            .press(press, repeat, now, &self.catalog, keys);
        self.quiet = true;
        self.hover(None, cx);
        self.pump(cx);
    }

    /// Tab moves the keyboard between the list and Empty Trash…, the strip's one live stop.
    pub fn focus_strip(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.interaction_held() {
            return;
        }
        self.host.view.strip_focus = on && self.empty_available();
        self.pump(cx);
    }
}
