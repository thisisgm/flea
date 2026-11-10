//! The destination reuses Entry and keeps its draft, selection and focus on the open menu session.
use super::{
    host::Overlay,
    model::{CopyFocus, Dialog},
};
use crate::path::{entry::Entry, field::Line};
use gpui::{
    ClipboardItem, Context, FocusHandle, KeyDownEvent, MouseDownEvent, MouseMoveEvent, Window,
};
use std::cell::RefCell;

#[derive(Default)]
pub(super) struct Editor {
    pub entry: Entry,
    pub focus: Option<FocusHandle>,
    restore: Option<FocusHandle>,
    menu: Option<u64>,
    pub drawn: RefCell<Drawn>,
}

#[derive(Default)]
pub(crate) struct Drawn {
    pub revealed: bool,
    pub held: usize,
    pub line: Line,
    // The exact argument retained with the current shaped run, replaced only when shaping changes.
    pub shaped_text: std::rc::Rc<str>,
    pub bounds: Option<gpui::Bounds<gpui::Pixels>>,
    pub font: Option<gpui::Font>,
    pub pixels: gpui::Pixels,
    #[cfg(test)]
    pub paint: Option<super::field::Paint>,
}

impl Editor {
    /// Forgets the editor's menu session and clears the draft
    pub fn restart(&mut self) {
        self.menu = None;
        self.entry.close();
    }
}

impl Overlay {
    /// Puts the keyboard on the card that just mounted, once its layer is in front.
    pub(crate) fn acquire_mount_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .editor
            .focus
            .as_ref()
            .is_some_and(|focus| focus.is_focused(window))
        {
            return;
        }
        if self.editor.restore.is_none() {
            self.editor.restore = window
                .focused(cx)
                .filter(|focus| self.editor.focus.as_ref() != Some(focus));
        }
        let focus = self.editor.focus.get_or_insert_with(|| cx.focus_handle());
        window.focus(focus, cx);
    }

    /// Hands focus back to whatever held it before the card, or blurs when nothing did
    pub(super) fn release_mount_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let restore = self.editor.restore.take();
        if self
            .editor
            .focus
            .as_ref()
            .is_some_and(|focus| focus.is_focused(window))
        {
            if let Some(restore) = restore {
                window.focus(&restore, cx);
            } else {
                window.blur(cx);
            }
        }
    }

    /// Points the shared entry at the open card's draft and moves the keyboard focus with the modal
    pub(super) fn sync_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.modal(cx) {
            self.acquire_mount_focus(window, cx);
        } else {
            self.release_mount_focus(window, cx);
        }
        if let Some((session, draft)) = self.model.dialog.as_ref().and_then(Dialog::draft) {
            if self.progress.dialog.holding(self.now(cx)) {
                return;
            }
            if !self.editor.entry.editing
                || self.editor.menu != Some(session)
                || self.editor.entry.text != *draft
            {
                self.editor.entry = Entry::default();
                self.editor.entry.begin(draft);
                self.editor.drawn.borrow_mut().line = Line::default();
            }
            self.editor.menu = Some(session);
        } else {
            self.editor.menu = None;
            self.editor.entry.close();
        }
        // A blur commits the composition as it stands, since the input method's late callbacks are refused.
        if !self.editing_copy(window, cx) {
            self.editor.entry.ime_unmark();
        }
    }

    // Whether the focused field owns the editor; each dialog field the editor serves is listed in Dialog::typing_session.
    /// Whether this card's field is the one focused, mounted and not held mid-progress
    pub(super) fn editing_copy(&self, window: &Window, cx: &gpui::App) -> bool {
        let Some(session) = self.model.dialog.as_ref().and_then(Dialog::typing_session) else {
            return false;
        };
        self.editor.menu == Some(session)
            && !self.progress.dialog.holding(self.now(cx))
            && self.editor.entry.editing
            && self
                .editor
                .focus
                .as_ref()
                .is_some_and(|f| f.is_focused(window))
    }

    // Platform callbacks reach the last registered handler, so they also need its session to be this one.
    /// Whether a platform text callback still belongs to the card's field and its shaped run
    pub(super) fn ime_field(&self, window: &Window, cx: &gpui::App) -> bool {
        self.editing_copy(window, cx) && self.editor.drawn.borrow().line.accepts(&self.editor.entry)
    }

    /// Copies the entry's text back into the open card's draft and redraws
    pub(super) fn edited_copy(&mut self, cx: &mut Context<Self>) {
        if let Some(draft) = self.model.dialog.as_mut().and_then(Dialog::draft_mut) {
            draft.clone_from(&self.editor.entry.text);
        }
        cx.notify();
    }

    // Text keys reach the platform handler; navigation and clipboard operations use the same Entry.
    /// Routes a key to the entry: the composition first, then cursor movement, erasing and the clipboard
    pub(super) fn copy_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.editing_copy(window, cx) {
            return false;
        }
        let keys = &event.keystroke;
        let key = keys.key.as_str();
        let held = keys.modifiers;
        let command = held.control || held.platform;
        // Enter belongs to the IME until its marked text commits; Escape cancels the composition, Tab commits it.
        if self.editor.entry.ime_key(key) {
            self.edited_copy(cx);
            cx.stop_propagation();
            return true;
        }
        let at = self.editor.entry.cursor();
        match key {
            "left" | "right" | "home" | "end" if !held.alt && !command => {
                let entry = &mut self.editor.entry;
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
            "backspace" | "delete" if !held.alt && !command => {
                self.editor.entry.erase(key == "delete")
            }
            "a" if command && !held.alt && !held.shift => self.editor.entry.select_all(),
            "c" | "x" if command && !held.alt && !held.shift => {
                if let Some(text) = self.editor.entry.copyable() {
                    cx.write_to_clipboard(ClipboardItem::new_string(text.to_owned()));
                    if key == "x" {
                        self.editor.entry.replace(None, "");
                    }
                }
            }
            _ if crate::window::input::paste_key(event) => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    let text = crate::window::input::clean_paste(&text);
                    self.editor.entry.replace(None, &text);
                }
            }
            "escape" | "enter" | "return" | "kp_enter" | "tab" | "backtab" => return false,
            _ if held.alt || command => return false,
            _ => return true,
        }
        self.edited_copy(cx);
        cx.stop_propagation();
        true
    }

    /// Places the caret, or extends the selection, at the press point, with double and triple clicks taking a word or all
    pub(super) fn copy_press(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.model.dialog.as_mut().and_then(Dialog::field_state) {
            Some((focus, false)) => *focus = CopyFocus::Field,
            _ => return,
        }
        if let Some(focus) = &self.editor.focus {
            window.focus(focus, cx);
        }
        let at = self.copy_index(event.position.x);
        let entry = &mut self.editor.entry;
        // The last drawn glyph line can precede an edit, so its hit must fit the current UTF-8 text.
        let at = entry.from_display(at);
        if event.click_count > 2 {
            entry.select_all();
        } else if event.click_count == 2 {
            let text = &entry.text;
            let separator = |c: char| c == '/' || c.is_whitespace();
            let start = text[..at]
                .char_indices()
                .rev()
                .find(|(_, c)| separator(*c))
                .map_or(0, |(at, c)| at + c.len_utf8());
            let end = text[at..]
                .find(separator)
                .map_or(text.len(), |end| at + end);
            entry.move_to(start);
            entry.select_to(end);
        } else if event.modifiers.shift {
            entry.select_to(at);
        } else {
            entry.move_to(at);
        }
        cx.stop_propagation();
        cx.notify();
    }

    /// Extends the selection to the pointer while the left button is dragged
    pub(super) fn copy_drag(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.pressed_button == Some(gpui::MouseButton::Left) && self.editing_copy(window, cx) {
            let at = self.copy_index(event.position.x);
            let at = self.editor.entry.from_display(at);
            self.editor.entry.select_to(at);
            cx.notify();
        }
    }

    /// The text index a window x falls on, from the last drawn line's bounds and scroll
    pub(super) fn copy_index(&self, x: gpui::Pixels) -> usize {
        let drawn = self.editor.drawn.borrow();
        let origin = drawn.bounds.map_or(gpui::px(0.), |b| b.left());
        drawn.line.index_for(x - origin + drawn.line.scroll)
    }
}
