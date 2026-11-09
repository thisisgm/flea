//! Add Plugin source composes text; schema Field rows remain display-only.
use super::{
    page::{Card, Key},
    view::PluginsPane,
};
use crate::g2::settings_pages::input::{input_handler, type_key, Editor, Host, KeyUse};
use gpui::{Context, FocusHandle, KeyDownEvent, Window};

impl Host for PluginsPane {
    fn editor(&self) -> &Editor {
        &self.editor
    }
    fn editor_mut(&mut self) -> &mut Editor {
        &mut self.editor
    }
    fn draft(&self) -> Option<&str> {
        if !self.input_enabled {
            return None;
        }
        match self.page.card() {
            Card::Add(card) if !card.busy => Some(&card.text),
            _ => None,
        }
    }
    fn input_focus(&self, window: &Window) -> &FocusHandle {
        if self.focus_handle().is_focused(window) {
            self.focus_handle()
        } else {
            &self.input_focus
        }
    }
    fn input_catalog(&self) -> &std::sync::Arc<flea::i18n::I18n> {
        &self.page.i18n
    }
    fn edited(&mut self, cx: &mut Context<Self>) {
        if let Some(text) = self.editor.publish() {
            if let Card::Add(card) = &mut self.page.card {
                card.text = text;
                card.error = None;
            }
        }
        cx.notify();
    }
}
input_handler!(PluginsPane);

impl PluginsPane {
    pub(crate) fn source_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !matches!(self.page.card(), Card::Add(_)) {
            return false;
        }
        self.sync_editor();
        if self.draft().is_none() {
            if event.keystroke.key == "escape" {
                self.key(Key::Escape, cx);
            }
            cx.stop_propagation();
            return true;
        }
        if self.text_menu_key(event, window, cx) {
            return true;
        }
        match self.editor.key(event, cx) {
            KeyUse::Text if !type_key(self, event, window, cx) => return true,
            KeyUse::Text => {}
            KeyUse::Handled => self.edited(cx),
            KeyUse::Command => match event.keystroke.key.as_str() {
                "escape" => self.key(Key::Escape, cx),
                "enter" => self.key(Key::Enter, cx),
                _ => {}
            },
        }
        cx.stop_propagation();
        true
    }

    pub(crate) fn mounted_input_focus(
        &mut self,
        focus: FocusHandle,
        enabled: bool,
        typeface: crate::list::Typeface,
    ) -> bool {
        let changed = self.input_focus != focus
            || self.input_enabled != enabled
            || self.input_typeface.font != typeface.font
            || self.input_typeface.body != typeface.body;
        self.input_focus = focus;
        self.input_typeface = typeface;
        self.input_enabled = enabled;
        self.sync_editor();
        changed
    }
}

impl PluginsPane {
    pub(crate) fn on_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.source_key(event, window, cx) {
            return;
        }
        let keystroke = &event.keystroke;
        if keystroke.key == "tab" {
            if *self.page.card() != Card::None {
                cx.stop_propagation();
            }
            return;
        }
        let plain = !keystroke.modifiers.control
            && !keystroke.modifiers.alt
            && !keystroke.modifiers.platform;
        let key =
            match crate::window::input::reading(keystroke.key.as_str(), window.layout_mirrored()) {
                "up" => Key::Up,
                "down" => Key::Down,
                "left" => Key::Left,
                "right" => Key::Right,
                "enter" => Key::Enter,
                "escape" => Key::Escape,
                "backspace" => Key::Backspace,
                "space" if !matches!(self.page.card(), Card::Add(_)) => Key::Space,
                _ => match (&keystroke.key_char, plain) {
                    (Some(text), true) => Key::Text(text.clone()),
                    _ => return,
                },
            };
        cx.stop_propagation();
        self.key(key, cx);
    }
}
