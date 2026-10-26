//! Modal keyboard dispatch for the network card and its suggestion menu.
use super::{dialog::Focus, view::NetworkView};
use crate::list::Typeface;
use gpui::{Context, KeyDownEvent, Window};

impl NetworkView {
    pub(super) fn key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.model.opened {
            return;
        }
        let k = &event.keystroke;
        let select_all = k.modifiers.control
            && k.key == "a"
            && (self.input.editing || self.model.focus == Focus::Password);
        let plain = !k.modifiers.control && !k.modifiers.alt && !k.modifiers.platform;
        if !select_all
            && !(plain
                && matches!(
                    k.key.as_str(),
                    "escape"
                        | "tab"
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
        if k.key == "escape" && self.menu.borrow().opened() {
            self.suggesting = false;
            self.changed(cx);
            return;
        }
        if k.key == "escape" {
            return self.close(cx);
        }
        if self.model.busy() {
            return;
        }
        if self.model.committed {
            match k.key.as_str() {
                "tab" => {
                    self.model.step(k.modifiers.shift);
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
        if k.modifiers.control && k.key == "a" && self.model.focus == Focus::Password {
            self.password_selection = 0..super::secret_edit::units(&self.model.password);
            self.password_reversed = false;
            self.changed(cx);
            return;
        }
        if k.modifiers.control && k.key == "a" && self.input.editing {
            self.input.select_all();
            self.changed(cx);
            return;
        }
        if k.modifiers.control || k.modifiers.alt || k.modifiers.platform {
            return;
        }
        if self.menu.borrow().opened() && matches!(k.key.as_str(), "down" | "up" | "enter") {
            let mut typeface = Typeface::omarchy_default();
            typeface.font = window.text_style().font();
            let action = match k.key.as_str() {
                "down" => "cursorDown",
                "up" => "cursorUp",
                _ => "open",
            };
            let outcome =
                self.menu
                    .borrow_mut()
                    .act(action, None, "", &self.env(&typeface), window);
            self.menu_outcome(outcome, cx);
            cx.notify();
            return;
        }
        match k.key.as_str() {
            "tab" => {
                self.model.step(k.modifiers.shift);
                self.load_input();
            }
            "down" if self.model.focus == Focus::Host => self.suggesting = true,
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
                let mut range = self.password_selection.clone();
                if range.is_empty() {
                    if k.key == "backspace" {
                        range.start =
                            super::secret_edit::previous(&self.model.password, range.start);
                    } else {
                        range.end = super::secret_edit::next(&self.model.password, range.end);
                    }
                }
                self.password_selection =
                    super::secret_edit::replace(&mut self.model.password, range, "");
                self.password_reversed = false;
                self.password_marked = None;
            }
            "left" | "right" | "home" | "end" if self.model.focus == Focus::Password => {
                let range = &self.password_selection;
                let cursor = if self.password_reversed {
                    range.start
                } else {
                    range.end
                };
                let at = match k.key.as_str() {
                    "left" if !k.modifiers.shift && !range.is_empty() => range.start,
                    "right" if !k.modifiers.shift && !range.is_empty() => range.end,
                    "left" => super::secret_edit::previous(&self.model.password, cursor),
                    "right" => super::secret_edit::next(&self.model.password, cursor),
                    "home" => 0,
                    _ => super::secret_edit::units(&self.model.password),
                };
                let anchor = if k.modifiers.shift {
                    if self.password_reversed {
                        range.end
                    } else {
                        range.start
                    }
                } else {
                    at
                };
                self.password_selection = anchor.min(at)..anchor.max(at);
                self.password_reversed = at < anchor;
                self.password_marked = None;
            }
            "backspace" | "delete" if self.input.editing => {
                self.input.erase(k.key == "delete");
                self.edited(cx);
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
        self.changed(cx);
    }
}
