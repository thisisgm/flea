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
        if matches!(name, "focusNext" | "focusPrevious") {
            self.host.view.arm.clear();
            return self.tab(name == "focusNext", cx);
        }
        if self.on_empty() && matches!(name, "open" | "activate") {
            self.host.view.prepare(true);
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

    /// Empty Trash…, the strip's one live stop, comes just before the list in the window's ring, as the strip draws above it.
    fn tab(&mut self, forward: bool, cx: &mut Context<Self>) {
        let on_strip = self.on_empty();
        if on_strip == forward && (on_strip || self.empty_available()) {
            return self.focus_strip(!on_strip, cx);
        }
        self.host.view.strip_focus = false;
        self.outside.leave(forward);
        self.pump(cx);
    }

    /// The window's ring enters Trash here: a forward step lands on Empty Trash… when it is live, a backward one on the list.
    pub fn focus_strip(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.interaction_held() {
            return;
        }
        self.host.view.strip_focus = on && self.empty_available();
        self.pump(cx);
    }
}
