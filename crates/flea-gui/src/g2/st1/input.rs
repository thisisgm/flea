//! Settings text fields share Entry, while their hosts retain search, ranking and installation decisions.
use crate::path::{entry::Entry, field::Line};
use flea::i18n::I18n;
use gpui::{ClipboardItem, Context, FocusHandle, KeyDownEvent, Window};
use std::cell::RefCell;
use std::sync::Arc;

mod field;
mod handler;
mod session;
pub(crate) use field::Field;
pub(crate) use handler::input_handler;

#[derive(Default)]
pub(crate) struct Editor {
    pub entry: Entry,
    pub drawn: RefCell<Drawn>,
    published: String,
    pub retired: bool,
}

#[derive(Default)]
pub(crate) struct Drawn {
    pub line: Line,
    pub bounds: Option<gpui::Bounds<gpui::Pixels>>,
    pub font: Option<gpui::Font>,
    pub pixels: gpui::Pixels,
    pub catalog: Option<Arc<I18n>>,
    #[cfg(test)]
    pub marked: Option<gpui::Bounds<gpui::Pixels>>,
}

#[derive(PartialEq)]
pub(crate) enum KeyUse {
    Text,
    Handled,
    Command,
}

pub(crate) trait Host: Sized + 'static {
    fn editor(&self) -> &Editor;
    fn editor_mut(&mut self) -> &mut Editor;
    fn draft(&self) -> Option<&str>;
    fn input_focus(&self, _: &Window) -> &FocusHandle;
    fn input_catalog(&self) -> &Arc<I18n>;
    fn edited(&mut self, cx: &mut Context<Self>);

    fn sync_editor(&mut self) {
        let editor = self.editor();
        let draft = self.draft();
        if editor.retired {
            return;
        }
        let changed = match draft {
            Some(text) => !editor.entry.editing || editor.published != text,
            None => editor.entry.editing,
        };
        if changed {
            let draft = draft.map(str::to_owned);
            let editor = self.editor_mut();
            editor.drawn.borrow_mut().line.registered = None;
            if let Some(text) = draft {
                editor.entry.begin(&text);
                editor.entry.move_to(text.len());
                editor.published = text;
            } else {
                editor.entry.close();
            }
        }
    }

    fn accepts_input(&self, window: &Window) -> bool {
        let editor = self.editor();
        !editor.retired
            && self.draft().is_some()
            && self.input_focus(window).is_focused(window)
            && editor.drawn.borrow().line.accepts(&editor.entry)
    }
}

/// Types a plain key as the platform would once the key bubbles unhandled, so a parent listener never takes it first.
pub(crate) fn type_key<T: Host + gpui::EntityInputHandler>(
    host: &mut T,
    event: &KeyDownEvent,
    window: &mut Window,
    cx: &mut Context<T>,
) -> bool {
    let stroke = &event.keystroke;
    let Some(text) = stroke.key_char.as_deref() else {
        return false;
    };
    if !stroke.modifiers.is_subset_of(&gpui::Modifiers::shift()) || !host.accepts_input(window) {
        return false;
    }
    host.replace_text_in_range(None, text, window, cx);
    true
}

impl Editor {
    pub fn retire(&mut self) {
        self.retired = true;
        self.entry.close();
        self.drawn.borrow_mut().line.registered = None;
    }

    /// Publish only committed text; a preedit cannot change search results or enable an installation.
    pub fn publish(&mut self) -> Option<String> {
        if self.entry.marked.is_some() || self.published == self.entry.text {
            return None;
        }
        self.published.clone_from(&self.entry.text);
        Some(self.published.clone())
    }

    pub fn clear(&mut self) {
        self.entry.begin("");
        self.published.clear();
        self.drawn.borrow_mut().line.registered = None;
    }

    pub fn key<T: 'static>(&mut self, event: &KeyDownEvent, cx: &mut Context<T>) -> KeyUse {
        let stroke = &event.keystroke;
        let key = stroke.key.as_str();
        let held = stroke.modifiers;
        if self.entry.marked.is_some() {
            if key == "escape" {
                self.entry.ime_key(key);
            }
            return KeyUse::Handled;
        }
        let at = self.entry.cursor();
        match key {
            "left" | "right" | "home" | "end" => {
                let to = match key {
                    "left" if !held.shift && !self.entry.selected.is_empty() => {
                        self.entry.selected.start
                    }
                    "right" if !held.shift && !self.entry.selected.is_empty() => {
                        self.entry.selected.end
                    }
                    "left" => self.entry.previous(at),
                    "right" => self.entry.next(at),
                    "home" => 0,
                    _ => self.entry.text.len(),
                };
                if held.shift {
                    self.entry.select_to(to);
                } else {
                    self.entry.move_to(to);
                }
            }
            "backspace" if self.entry.text.is_empty() => return KeyUse::Command,
            "backspace" | "delete" => self.entry.erase(key == "delete"),
            "a" | "c" | "x" | "v" if held.control && !held.alt && !held.platform => match key {
                "a" => self.entry.select_all(),
                "v" => {
                    if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                        self.entry.ime_replace(None, &text);
                    }
                }
                _ => {
                    if let Some(text) = self.entry.copyable() {
                        cx.write_to_clipboard(ClipboardItem::new_string(text.to_owned()));
                        if key == "x" {
                            self.entry.replace(None, "");
                        }
                    }
                }
            },
            _ if held.platform || (held.control && !held.alt) => return KeyUse::Handled,
            "escape" | "enter" | "return" | "kp_enter" | "tab" | "backtab" | "up" | "down" => {
                return KeyUse::Command
            }
            _ if stroke.key_char.is_some() || stroke.is_ime_in_progress() => return KeyUse::Text,
            _ => return KeyUse::Command,
        }
        KeyUse::Handled
    }
}
