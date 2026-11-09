//! Octal editing uses the same entry session, selection, caret, clipboard and composition as the other card fields.
use super::{Phase, Stop, View};
use gpui::{ClipboardItem, Context, KeyDownEvent, Window};

impl View {
    /// The octal text while the field takes typing: one item, the field focused and its mode changeable.
    fn octal_value(&self) -> Option<&str> {
        (self.opened
            && self.model.focus == Stop::Octal
            && self.model.phase == Phase::Ready
            && self.model.changeable())
        .then_some(self.model.text.as_str())
    }

    /// Each focus on the field opens a new session over the text; leaving it ends the session, so late callbacks miss.
    pub(super) fn sync_editor(&mut self) {
        let Some(value) = self.octal_value() else {
            self.editor.close();
            self.drawn.borrow_mut().line.registered = None;
            return;
        };
        if !self.editor.editing || self.editor.text != value {
            let value = value.to_owned();
            self.editor.kind = crate::path::entry::Kind::Digits;
            self.editor.begin(&value);
            self.drawn.borrow_mut().line = Default::default();
        }
    }

    pub(super) fn editing_octal(&self, window: &Window) -> bool {
        self.octal_value().is_some() && self.editor.editing && self.focus.is_focused(window)
    }

    pub(super) fn edited(&mut self, cx: &mut Context<Self>) {
        self.model.text.clone_from(&self.editor.text);
        self.flush(Vec::new(), cx);
    }

    /// The field's own keys; typed text reaches it through the platform, and Return, Tab and Escape reach the card.
    pub(super) fn edit_octal(
        &mut self,
        event: &KeyDownEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.sync_editor();
        if !self.editing_octal(window) {
            return false;
        }
        let stroke = &event.keystroke;
        let key = stroke.key.as_str();
        let held = stroke.modifiers;
        let command = held.control || held.platform;
        let entry = &mut self.editor;
        if !entry.ime_key(key) {
            let at = entry.cursor();
            match key {
                "left" | "right" | "home" | "end" if !held.alt && !command => {
                    let to = match key {
                        "left" if !held.shift && !entry.selected.is_empty() => entry.selected.start,
                        "right" if !held.shift && !entry.selected.is_empty() => entry.selected.end,
                        "left" => entry.previous(at),
                        "right" => entry.next(at),
                        "home" => 0,
                        _ => entry.text.len(),
                    };
                    if held.shift {
                        entry.select_to(to);
                    } else {
                        entry.move_to(to);
                    }
                }
                "backspace" | "delete" if !held.alt && !command => entry.erase(key == "delete"),
                "a" if command && !held.alt && !held.shift => entry.select_all(),
                "c" | "x" if command && !held.alt && !held.shift => {
                    if let Some(text) = entry.copyable() {
                        cx.write_to_clipboard(ClipboardItem::new_string(text.to_owned()));
                        if key == "x" {
                            entry.ime_replace(None, "");
                        }
                    }
                }
                _ if crate::window::input::paste_key(event) => {
                    if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                        entry.ime_replace(None, &crate::window::input::clean_paste(&text));
                    }
                }
                "escape" | "enter" | "return" | "kp_enter" | "tab" | "backtab" | "up" | "down" => {
                    return false
                }
                _ if command || held.alt => return false,
                _ => return true,
            }
        }
        cx.stop_propagation();
        self.edited(cx);
        true
    }
}
