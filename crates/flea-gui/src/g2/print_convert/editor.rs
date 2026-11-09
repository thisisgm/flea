//! The active output field uses the shared entry model and painted line, scoped to its card and focus.
use super::{
    host::OutputView,
    model::{Card, Focus},
};
use crate::{
    g2::file_ops::{editor::Drawn, field_view::FieldView},
    path::entry::Entry,
};
use gpui::{
    App, ClipboardItem, Context, FocusHandle, KeyDownEvent, MouseDownEvent, MouseMoveEvent, Window,
};
use std::cell::RefCell;

#[derive(Default)]
pub(super) struct Editor {
    pub entry: Entry,
    pub drawn: RefCell<Drawn>,
    pub owner: Option<Focus>,
    pub face: Option<crate::list::Typeface>,
}

impl Editor {
    pub fn close(&mut self) {
        self.entry.close();
        self.owner = None;
        self.drawn.borrow_mut().line.registered = None;
    }
}

impl OutputView {
    pub(super) fn field_value(&self) -> Option<&str> {
        if !self.model.editable() || self.printer_menu.opened() {
            return None;
        }
        match (&self.model.card, self.model.focus) {
            (Some(Card::Print(p)), Some(Focus::Copies))
                if p.queried && p.pending.is_none() && !p.destinations.is_empty() =>
            {
                Some(&p.copies)
            }
            (Some(Card::Print(p)), Some(Focus::Pages))
                if p.queried && p.pending.is_none() && !p.destinations.is_empty() =>
            {
                Some(&p.pages)
            }
            (Some(Card::Convert(c)), Some(Focus::Pages)) => Some(&c.pages),
            (Some(Card::Combine(c)), Some(Focus::Name)) => Some(&c.name),
            (Some(Card::Combine(c)), Some(Focus::Columns)) if c.grid() => Some(&c.columns_text),
            _ => None,
        }
    }

    pub(super) fn sync_editor(&mut self) {
        let Some(value) = self.field_value() else {
            self.editor.close();
            return;
        };
        if self.editor.owner != self.model.focus
            || !self.editor.entry.editing
            || self.editor.entry.text != value
        {
            let value = value.to_owned();
            self.editor.entry.kind =
                if matches!(self.model.focus, Some(Focus::Copies | Focus::Columns)) {
                    crate::path::entry::Kind::Digits
                } else {
                    crate::path::entry::Kind::Text
                };
            self.editor.entry.begin(&value);
            self.editor.drawn.borrow_mut().line = Default::default();
            self.editor.owner = self.model.focus;
        }
    }

    pub(super) fn editing_field(&self, window: &Window) -> bool {
        self.field_value().is_some()
            && self.editor.owner == self.model.focus
            && self.editor.entry.editing
            && self.focus.is_focused(window)
    }

    pub(super) fn ime_field(&self, window: &Window, _: &App) -> bool {
        self.editing_field(window) && self.editor.drawn.borrow().line.accepts(&self.editor.entry)
    }

    pub(super) fn edited_field(&mut self, cx: &mut Context<Self>) {
        let value = &self.editor.entry.text;
        match (&mut self.model.card, self.model.focus) {
            (Some(Card::Print(p)), Some(Focus::Copies)) => p.copies.clone_from(value),
            (Some(Card::Print(p)), Some(Focus::Pages)) => p.pages.clone_from(value),
            (Some(Card::Convert(c)), Some(Focus::Pages)) => c.pages.clone_from(value),
            (Some(Card::Combine(c)), Some(Focus::Name)) => {
                c.custom_name = true;
                c.name.clone_from(value);
            }
            (Some(Card::Combine(c)), Some(Focus::Columns)) if c.grid() => {
                c.columns_text.clone_from(value)
            }
            _ => return,
        }
        self.validate_edit(cx);
    }

    pub(super) fn field_key(
        &mut self,
        event: &KeyDownEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.editing_field(window) {
            return false;
        }
        let key = event.keystroke.key.as_str();
        let held = event.keystroke.modifiers;
        let command = held.control || held.platform;
        let revision = self.editor.entry.revision;
        if self.editor.entry.ime_key(key) {
            cx.stop_propagation();
            if revision != self.editor.entry.revision {
                self.edited_field(cx);
            } else {
                cx.notify();
            }
            return true;
        }
        let entry = &mut self.editor.entry;
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
                    let text = crate::window::input::clean_paste(&text);
                    if entry.text.len() - entry.selected.len() + text.len() <= 4096 {
                        entry.ime_replace(None, &text);
                    }
                }
            }
            "escape" | "enter" | "return" | "kp_enter" | "tab" | "backtab" => return false,
            _ if command || held.alt => return false,
            _ => return true,
        }
        cx.stop_propagation();
        if revision != self.editor.entry.revision {
            self.edited_field(cx);
        } else {
            cx.notify();
        }
        true
    }
}

impl FieldView for OutputView {
    fn field_entry(&self) -> &Entry {
        &self.editor.entry
    }
    fn field_drawn(&self) -> &RefCell<Drawn> {
        &self.editor.drawn
    }
    fn field_active(&self, window: &Window, _: &App) -> bool {
        self.editing_field(window)
    }
    fn field_palette(&self) -> crate::g2::file_ops::palette::CardPalette {
        self.palette
    }
    fn field_focus(&self) -> Option<FocusHandle> {
        Some(self.focus.clone())
    }
    fn field_press(&mut self, event: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let drawn = self.editor.drawn.borrow();
        let left = drawn.bounds.map_or(gpui::px(0.), |b| b.left());
        let at =
            crate::path::field::ime_index(&drawn.line, &self.editor.entry, event.position.x - left);
        let at = self.editor.entry.offset_from_utf16(at);
        let x = event.position.x - left + drawn.line.scroll;
        if event.click_count > 2 {
            self.editor.entry.select_all();
        } else if event.click_count == 2 {
            self.editor.entry.select_word_under(&drawn.line, x);
        } else if event.modifiers.shift {
            self.editor.entry.select_to(at);
        } else {
            self.editor.entry.move_to(at);
        }
        cx.stop_propagation();
        cx.notify();
    }
    fn field_drag(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.pressed_button != Some(gpui::MouseButton::Left) || !self.editing_field(window) {
            return;
        }
        let drawn = self.editor.drawn.borrow();
        let left = drawn.bounds.map_or(gpui::px(0.), |b| b.left());
        let at =
            crate::path::field::ime_index(&drawn.line, &self.editor.entry, event.position.x - left);
        let at = self.editor.entry.offset_from_utf16(at);
        drop(drawn);
        self.editor.entry.select_to(at);
        cx.notify();
    }
}
