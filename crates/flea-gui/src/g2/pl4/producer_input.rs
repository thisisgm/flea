//! Rail editor and keyboard actions, dispatched by the window focus owner.
use super::*;

impl Producer {
    /// F2 only starts on a network share, the rows Sidebar.qml gives a rail editor.
    pub fn start_rename(&mut self, index: usize, cx: &mut Context<Self>) -> bool {
        let Some(entry) = self.entry(index) else {
            return false;
        };
        if entry.group != Group::Network || entry.error.is_some() {
            return false;
        }
        let mut model = self.model.borrow_mut();
        model.renaming = Some(index);
        model.editor.begin(&entry.label);
        self.editor_dragging = false;
        drop(model);
        cx.notify();
        true
    }

    pub fn rename_push(&mut self, text: &str, cx: &mut Context<Self>) {
        let mut model = self.model.borrow_mut();
        if model.renaming.is_none() {
            return;
        }
        let text: String = text.chars().filter(|ch| !ch.is_control()).collect();
        model.editor.replace(None, &text);
        drop(model);
        cx.notify();
    }

    pub fn rename_backspace(&mut self, cx: &mut Context<Self>) {
        let mut model = self.model.borrow_mut();
        if model.renaming.is_none() {
            return;
        }
        model.editor.erase(false);
        drop(model);
        cx.notify();
    }

    /// Commits as a request to the owner of the bookmarks file; the rail never writes persistence itself.
    pub fn commit_rename(&mut self, cx: &mut Context<Self>) {
        let index = self.model.borrow().renaming;
        let Some(index) = index else {
            return;
        };
        if self.model.borrow().editor.marked.is_some() {
            return;
        }
        let label = self.model.borrow().editor.text.trim().to_owned();
        let original = self.entry(index);
        if !label.is_empty() {
            if let Some(entry) = original.filter(|entry| entry.label != label) {
                self.queue(Event::Rename { entry, label });
            }
        }
        self.model.borrow_mut().renaming = None;
        self.model.borrow_mut().editor.close();
        self.editor_dragging = false;
        cx.notify();
    }

    pub fn cancel_rename(&mut self, cx: &mut Context<Self>) {
        self.editor_dragging = false;
        let cancelled = self.model.borrow_mut().renaming.take().is_some();
        if cancelled {
            self.model.borrow_mut().editor.close();
            cx.notify();
        }
    }

    pub(super) fn editor_press(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        if event.button != MouseButton::Left {
            return;
        }
        let mut model = self.model.borrow_mut();
        let rect = model
            .renaming
            .and_then(|index| self.laid.borrow().row(index)?.editor);
        let Some(rect) = rect else { return };
        let x = f32::from(event.position.x);
        let y = f32::from(event.position.y);
        if x < rect.x || x >= rect.x + rect.width || y < rect.y || y >= rect.y + rect.height {
            return;
        }
        let at = model.editor_index(x - rect.x - 4. + model.editor_scroll);
        match event.click_count {
            1 if event.modifiers.shift => model.editor.select_to(at),
            1 => model.editor.move_to(at),
            2 => {
                // A drawn hit can precede an edit; Entry bounds it to the current UTF-8 text.
                model.editor.move_to(at);
                let at = model.editor.cursor();
                let word = |ch: char| ch != '/' && !ch.is_whitespace();
                let text = &model.editor.text;
                let start = text[..at]
                    .char_indices()
                    .rfind(|(_, ch)| !word(*ch))
                    .map_or(0, |(at, ch)| at + ch.len_utf8());
                let end = text[at..]
                    .find(|ch: char| !word(ch))
                    .map_or(text.len(), |end| at + end);
                model.editor.move_to(start);
                model.editor.select_to(end);
            }
            _ => model.editor.select_all(),
        }
        self.editor_dragging = true;
        drop(model);
        cx.notify();
    }

    pub(super) fn editor_drag(&mut self, event: &gpui::MouseMoveEvent, cx: &mut Context<Self>) {
        if event.pressed_button != Some(MouseButton::Left) || !self.editor_dragging {
            return;
        }
        let mut model = self.model.borrow_mut();
        let rect = model
            .renaming
            .and_then(|index| self.laid.borrow().row(index)?.editor);
        let Some(rect) = rect else { return };
        let at =
            model.editor_index(f32::from(event.position.x) - rect.x - 4. + model.editor_scroll);
        model.editor.select_to(at);
        drop(model);
        cx.stop_propagation();
        cx.notify();
    }

    pub fn rename_key(&mut self, event: &gpui::KeyDownEvent, cx: &mut Context<Self>) -> bool {
        use gpui::ClipboardItem;
        let stroke = &event.keystroke;
        let held = stroke.modifiers;
        let key = stroke.key.as_str();
        {
            let mut model = self.model.borrow_mut();
            if model.renaming.is_none() {
                return false;
            }
            if model.editor.ime_key(key) {
                cx.notify();
                return true;
            }
            if model.editor.marked.is_some() {
                return false;
            }
        }
        if key == "escape" {
            self.cancel_rename(cx);
            return true;
        }
        if key == "enter" {
            self.commit_rename(cx);
            return true;
        }
        let mut model = self.model.borrow_mut();
        let entry = &mut model.editor;
        match key {
            "backspace" | "delete" => entry.erase(key == "delete"),
            "left" | "right" | "home" | "end" => {
                let to = match key {
                    "left" if !held.shift && !entry.selected.is_empty() => entry.selected.start,
                    "right" if !held.shift && !entry.selected.is_empty() => entry.selected.end,
                    "left" => entry.previous(entry.cursor()),
                    "right" => entry.next(entry.cursor()),
                    "home" => 0,
                    _ => entry.text.len(),
                };
                if held.shift {
                    entry.select_to(to);
                } else {
                    entry.move_to(to);
                }
            }
            "v" if held.control => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    entry.replace(
                        None,
                        &text
                            .chars()
                            .filter(|ch| !ch.is_control())
                            .collect::<String>(),
                    );
                }
            }
            "a" if held.control => entry.select_all(),
            "c" | "x" if held.control && !entry.selected.is_empty() => {
                cx.write_to_clipboard(ClipboardItem::new_string(
                    entry.text[entry.selected.clone()].to_owned(),
                ));
                if key == "x" {
                    entry.replace(None, "");
                }
            }
            _ if stroke.key_char.is_some() && !held.control && !held.platform => return false,
            _ => {}
        }
        drop(model);
        cx.notify();
        true
    }

    pub fn key_action(&mut self, action: &str, cx: &mut Context<Self>) {
        let cursor = self.cursor();
        let index = cursor.unwrap_or(0);
        let last = self.count().saturating_sub(1);
        self.model.borrow_mut().pointer_focus = false;
        // A cursor key leaves the mark for the rows, even where the cursor cannot move.
        if action.starts_with("cursor") {
            self.model.borrow_mut().mark_focused = false;
        }
        match action {
            "cursorDown" => {
                self.model.borrow_mut().cursor = Some(cursor.map_or(0, |i| (i + 1).min(last)))
            }
            "cursorUp" => self.model.borrow_mut().cursor = Some(index.saturating_sub(1)),
            "cursorFirst" => self.model.borrow_mut().cursor = Some(0),
            "cursorLast" => self.model.borrow_mut().cursor = Some(last),
            // Focus.js: l (pageForward) on the rail opens its row, as Enter does.
            "open" | "pageForward" if cursor.is_some() => self.activate(index, cx),
            "rename" if cursor.is_some() => {
                self.start_rename(index, cx);
            }
            "menu" if cursor.is_some() => {
                if let Some(entry) = self.entry(index) {
                    self.queue(Event::Menu(entry));
                }
            }
            _ => {}
        }
        self.model.borrow_mut().reveal_cursor = true;
        cx.notify();
    }

    /// The unlock host lands the cursor on the unlocked row, as Sidebar.cursorIndex.
    pub fn set_cursor(&mut self, index: usize, cx: &mut Context<Self>) {
        self.model.borrow_mut().cursor = Some(index);
        cx.notify();
    }
}
