//! Modal keyboard dispatch for the network card.
use super::{dialog::Focus, view::NetworkView};
use gpui::{Context, KeyDownEvent, Window};

impl NetworkView {
    pub(super) fn key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.model.opened {
            return;
        }
        let k = &event.keystroke;
        if self.input.ime_key(&k.key) {
            self.sync_password_state();
            self.edited(cx);
            self.changed(cx);
            cx.stop_propagation();
            return;
        }

        let select_all = k.modifiers.control
            && k.key == "a"
            && (self.input.editing || self.model.focus == Focus::Password);
        let plain = !k.modifiers.control && !k.modifiers.alt && !k.modifiers.platform;
        if plain && k.key == "space" && self.input.editing {
            return;
        }
        if !select_all
            && !(plain
                && matches!(
                    k.key.as_str(),
                    "escape"
                        | "tab"
                        | "backtab"
                        | "enter"
                        | "space"
                        | "backspace"
                        | "delete"
                        | "left"
                        | "right"
                        | "home"
                        | "end"
                        | "down"
                        | "up"
                ))
        {
            return;
        }
        // A space typed into a text field reaches the input handler as text.
        if k.key == "space"
            && plain
            && (self.input.editing || self.model.focus == Focus::Password)
            && !self.model.committed
        {
            return;
        }
        cx.stop_propagation();
        if k.key == "escape" {
            return self.close(cx);
        }
        if self.model.busy() {
            return;
        }
        if self.model.committed {
            match k.key.as_str() {
                "tab" | "backtab" => {
                    self.model.step(k.modifiers.shift || k.key == "backtab");
                    self.load_input();
                    self.changed(cx);
                }
                "enter" | "space"
                    if matches!(
                        self.model.focus,
                        Focus::Cancel | Focus::Connect | Focus::Dropbox
                    ) =>
                {
                    self.activate(self.model.focus, cx)
                }
                "enter" => {
                    let effects = self.model.submit();
                    self.effects(effects, cx);
                }
                _ => {}
            }
            return;
        }
        if k.modifiers.control && k.key == "a" && self.input.editing {
            self.input.select_all();
            self.sync_password_state();
            self.changed(cx);
            return;
        }
        if k.modifiers.control || k.modifiers.alt || k.modifiers.platform {
            return;
        }
        match k.key.as_str() {
            "tab" | "backtab" => {
                self.model.step(k.modifiers.shift || k.key == "backtab");
                self.load_input();
            }
            "enter" => {
                let focus = self.model.focus;
                if matches!(
                    focus,
                    Focus::Protocol(_)
                        | Focus::Eye
                        | Focus::Tls
                        | Focus::Cancel
                        | Focus::Connect
                        | Focus::Dropbox
                ) {
                    self.activate(focus, cx);
                } else {
                    let effects = self.model.submit();
                    self.effects(effects, cx);
                }
            }
            "space" if !self.input.editing && self.model.focus != Focus::Password => {
                self.activate(self.model.focus, cx)
            }
            "backspace" | "delete" if self.model.focus == Focus::Password => {
                crate::g2::pl1::private_entry::erase(
                    &mut self.input,
                    &mut self.model.password,
                    k.key == "delete",
                );
                self.sync_password_state();
            }
            "backspace" | "delete" if self.input.editing => {
                self.input.erase(k.key == "delete");
                self.edited(cx);
            }
            "left" | "right" if self.model.focus == Focus::Password => {
                let units = self.input.offset_to_utf16(self.input.cursor());
                let units = if k.key == "left" {
                    super::secret_edit::previous(&self.model.password, units)
                } else {
                    super::secret_edit::next(&self.model.password, units)
                };
                let at = self.input.offset_from_utf16(units);
                if k.modifiers.shift {
                    self.input.select_to(at);
                } else {
                    self.input.move_to(at);
                }
            }
            "left" | "right" | "home" | "end" if self.input.editing => {
                let at = match k.key.as_str() {
                    "left" => self.input.previous(self.input.cursor()),
                    "right" => self.input.next(self.input.cursor()),
                    "home" => 0,
                    _ => self.input.text.len(),
                };
                if k.modifiers.shift {
                    self.input.select_to(at);
                } else {
                    self.input.move_to(at);
                }
            }
            _ => {}
        }
        self.sync_password_state();
        self.changed(cx);
    }
}
