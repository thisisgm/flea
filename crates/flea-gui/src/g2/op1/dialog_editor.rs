//! Operation hosts share one focused entry session while keeping their existing dispatch and card models.
use super::{editor::Drawn, field_view::FieldView, palette::CardPalette};
use crate::{
    path::entry::{Entry, Kind},
    state::Shell,
};
use gpui::{App, Context, FocusHandle, KeyDownEvent, MouseDownEvent, MouseMoveEvent, Window};
use std::cell::RefCell;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Slot {
    ArchiveName,
    ArchivePassword,
    ArchiveConfirm,
    ExtractPassword,
    BulkFind,
    BulkReplace,
    BulkLine(usize),
    PluginAction,
    OpenWithSearch,
}

#[derive(Default)]
pub(crate) struct DialogEditor {
    pub entry: Entry,
    pub drawn: RefCell<Drawn>,
    pub owner: Option<Slot>,
    pub palette: Option<CardPalette>,
    pub error_ink: bool,
}

impl DialogEditor {
    pub fn close(&mut self) {
        self.entry.close();
        self.owner = None;
        self.drawn.borrow_mut().line.registered = None;
    }
}

impl Shell {
    /// A card whose fields this shared session serves is showing, so the inline name underneath takes no callback.
    pub(crate) fn dialog_showing(&self) -> bool {
        self.open_with.is_some()
            || self.operations.archive.showing()
            || self.rename.bulk.is_some()
            || self.plugins.input.is_some()
    }

    pub(crate) fn dialog_value(&self) -> Option<(Slot, &str)> {
        if let Some(open) = self.plugins.input.as_ref().filter(|open| open.focus == 0) {
            return Some((Slot::PluginAction, &open.value));
        }
        if let Some(open) = &self.open_with {
            return open.card.ime_value();
        }
        let archive = &self.operations.archive;
        if let Some(card) = archive.compress.as_ref().filter(|c| c.opened()) {
            return card.ime_value();
        }
        if archive.extract.opened() {
            return archive.extract.ime_value();
        }
        self.rename
            .bulk
            .as_ref()
            .and_then(|open| open.card.ime_value())
    }

    pub(crate) fn sync_dialog_editor(&mut self) {
        if self.dialog_showing() {
            if let Some(edit) = &mut self.rename.edit {
                edit.entry.ime_unmark();
                edit.line.line.registered = None;
            }
        }
        let Some((slot, value)) = self.dialog_value() else {
            self.rename.ime.close();
            return;
        };
        if self.rename.ime.owner != Some(slot)
            || !self.rename.ime.entry.editing
            || self.rename.ime.entry.committed() != value
        {
            let value = value.to_owned();
            let editor = &mut self.rename.ime;
            editor.entry.kind = if matches!(
                slot,
                Slot::ArchivePassword | Slot::ArchiveConfirm | Slot::ExtractPassword
            ) {
                Kind::Private
            } else {
                Kind::Text
            };
            editor.entry.begin(&value);
            editor.drawn.borrow_mut().line = Default::default();
            editor.owner = Some(slot);
        }
    }

    pub(crate) fn dialog_active(&self, window: &Window) -> bool {
        self.dialog_value()
            .is_some_and(|(slot, _)| self.rename.ime.owner == Some(slot))
            && self.rename.ime.entry.editing
            && self.focus.is_focused(window)
    }

    pub(crate) fn dialog_edited(&mut self, cx: &mut Context<Self>) {
        let editor = &self.rename.ime;
        if let Some(slot) = editor.owner {
            // Cards act on committed text only; the field itself still paints the preedit.
            let committed = editor.entry.committed();
            let text = &*committed;
            if let Some(open) = self
                .plugins
                .input
                .as_mut()
                .filter(|_| slot == Slot::PluginAction)
            {
                open.value = text.chars().take(open.descriptor.max_chars).collect();
            } else if let Some(open) = self.open_with.as_mut() {
                open.card.ime_set(text, editor.entry.marked.is_some());
            } else if let Some(card) = self
                .operations
                .archive
                .compress
                .as_mut()
                .filter(|c| c.opened())
            {
                card.ime_set(slot, text);
            } else if self.operations.archive.extract.opened() {
                self.operations.archive.extract.ime_set(text);
            } else if let Some(open) = self.rename.bulk.as_mut() {
                open.card.ime_set(slot, text);
            }
        }
        cx.notify();
    }

    pub(crate) fn dialog_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        self.sync_dialog_editor();
        if self.rename.ime.owner.is_none() {
            return false;
        }
        let key = event.keystroke.key.as_str();
        let held = event.keystroke.modifiers;
        let command = held.control || held.platform;
        let composing = self.rename.ime.entry.marked.is_some();
        if self.rename.ime.entry.ime_key(key) {
            cx.stop_propagation();
            self.dialog_edited(cx);
            return true;
        }
        if composing && self.rename.ime.entry.marked.is_none() {
            // Tab committed the composition, so the card reads the line before focus moves.
            self.dialog_edited(cx);
        }
        let entry = &mut self.rename.ime.entry;
        let at = entry.cursor();
        match key {
            "left" | "right" | "home" | "end" if !command && !held.alt => {
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
            "backspace" | "delete" if !command && !held.alt => entry.erase(key == "delete"),
            "a" if command && !held.alt => entry.select_all(),
            "c" | "x" if command && !held.alt => {
                if let Some(text) = entry.copyable() {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(text.to_owned()));
                    if key == "x" {
                        entry.ime_replace(None, "");
                    }
                }
            }
            _ if crate::window::input::paste_key(event) => {
                if let Some(text) = cx.read_from_clipboard().and_then(|i| i.text()) {
                    entry.ime_replace(None, &crate::window::input::clean_paste(&text));
                }
            }
            "escape" | "enter" | "return" | "kp_enter" | "tab" | "backtab" | "up" | "down" => {
                return false
            }
            _ if command || held.alt => return false,
            _ => return true,
        }
        cx.stop_propagation();
        self.dialog_edited(cx);
        true
    }
}

impl FieldView for Shell {
    fn field_entry(&self) -> &Entry {
        &self.rename.ime.entry
    }
    fn field_drawn(&self) -> &RefCell<Drawn> {
        &self.rename.ime.drawn
    }
    fn field_active(&self, window: &Window, _: &App) -> bool {
        self.dialog_active(window)
    }
    fn field_palette(&self) -> CardPalette {
        self.rename.ime.palette.expect("mounted card palette")
    }
    fn field_focus(&self) -> Option<FocusHandle> {
        Some(self.focus.clone())
    }
    fn field_ink(&self) -> gpui::Hsla {
        let palette = self.field_palette();
        if self.rename.ime.error_ink {
            palette.error
        } else {
            palette.foreground
        }
    }
    fn field_held(&self) -> usize {
        if self.rename.ime.owner == Some(Slot::ExtractPassword) {
            self.operations.archive.extract.held_marks()
        } else {
            0
        }
    }
    fn field_revealed(&self) -> bool {
        self.operations
            .archive
            .compress
            .as_ref()
            .filter(|card| card.opened())
            .map_or_else(
                || {
                    self.operations.archive.extract.opened()
                        && self.operations.archive.extract.ime_revealed()
                },
                |card| card.ime_revealed(),
            )
    }
    fn field_press(&mut self, event: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let drawn = self.rename.ime.drawn.borrow();
        let left = drawn.bounds.map_or(gpui::px(0.), |b| b.left());
        let at = super::field_view::index(self, event.position.x - left);
        drop(drawn);
        if event.click_count > 1 {
            self.rename.ime.entry.select_all();
        } else if event.modifiers.shift {
            self.rename.ime.entry.select_to(at);
        } else {
            self.rename.ime.entry.move_to(at);
        }
        cx.stop_propagation();
        cx.notify();
    }
    fn field_drag(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.pressed_button != Some(gpui::MouseButton::Left) || !self.dialog_active(window) {
            return;
        }
        let drawn = self.rename.ime.drawn.borrow();
        let left = drawn.bounds.map_or(gpui::px(0.), |b| b.left());
        let at = super::field_view::index(self, event.position.x - left);
        drop(drawn);
        self.rename.ime.entry.select_to(at);
        cx.notify();
    }
}
