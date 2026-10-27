//! Octal editing uses the same selection, caret and clipboard operations as the path entry.
use super::{key_of, Key, Phase, Stop, View};
use gpui::{ClipboardItem, Context, KeyDownEvent};

impl View {
    pub(super) fn edit_octal(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        if self.model.focus != Stop::Octal
            || self.model.phase != Phase::Ready
            || !self.model.changeable()
        {
            return false;
        }
        let entry = &mut self.editor;
        if entry.text != self.model.text {
            entry.text.clone_from(&self.model.text);
            entry.move_to(entry.text.len());
        }
        let stroke = &event.keystroke;
        let key = stroke.key.as_str();
        let held = stroke.modifiers;
        let command = held.control || held.platform;
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
                if !entry.selected.is_empty() {
                    cx.write_to_clipboard(ClipboardItem::new_string(
                        entry.text[entry.selected.clone()].to_string(),
                    ));
                    if key == "x" {
                        entry.replace(None, "");
                    }
                }
            }
            _ if crate::window::input::paste_key(event) => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    entry.replace(None, &crate::window::input::clean_paste(&text));
                }
            }
            _ => match key_of(event) {
                Some(Key::Text(text)) => entry.replace(None, &text),
                Some(Key::Space) => entry.replace(None, " "),
                _ => return false,
            },
        }
        self.model.text.clone_from(&entry.text);
        true
    }
}
