//! Only actual editable hosts participate in platform text input.
use super::input::{input_handler, type_key, Editor, Field, Host, KeyUse};
use super::{
    geometry::{self as g, Scale},
    marks::mark,
    seams::{Ink, Request},
};
use super::{SettingsView, SheetView};
use crate::list::Typeface;
use gpui::{div, prelude::*, px, Context, Div, FocusHandle, KeyDownEvent, Window};

impl Host for SettingsView {
    fn editor(&self) -> &Editor {
        &self.editor
    }
    fn editor_mut(&mut self) -> &mut Editor {
        &mut self.editor
    }
    fn draft(&self) -> Option<&str> {
        self.panel.query.as_deref()
    }
    fn input_focus(&self, _: &Window) -> &FocusHandle {
        &self.focus
    }
    fn input_catalog(&self) -> &std::sync::Arc<flea::i18n::I18n> {
        &self.panel.catalog
    }
    fn edited(&mut self, cx: &mut Context<Self>) {
        if let Some(text) = self.editor.publish() {
            self.panel.query_changed(text);
        }
        cx.notify();
    }
}
input_handler!(SettingsView);

impl SettingsView {
    pub(super) fn query_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.panel.query.is_none() {
            return false;
        }
        self.sync_editor();
        match self.editor.key(event, cx) {
            KeyUse::Text if !type_key(self, event, window, cx) => return true,
            KeyUse::Text => {}
            KeyUse::Handled => self.edited(cx),
            KeyUse::Command => {
                self.panel
                    .key(&event.keystroke.key, "", event.keystroke.modifiers.shift);
                self.flush(cx);
            }
        }
        cx.stop_propagation();
        true
    }
}

impl Host for SheetView {
    fn editor(&self) -> &Editor {
        &self.editor
    }
    fn editor_mut(&mut self) -> &mut Editor {
        &mut self.editor
    }
    fn draft(&self) -> Option<&str> {
        Some(&self.sheet.query)
    }
    fn input_focus(&self, _: &Window) -> &FocusHandle {
        &self.focus
    }
    fn input_catalog(&self) -> &std::sync::Arc<flea::i18n::I18n> {
        &self.sheet.catalog
    }
    fn edited(&mut self, cx: &mut Context<Self>) {
        if let Some(text) = self.editor.publish() {
            self.sheet.query_changed(text);
        }
        cx.notify();
    }
}
input_handler!(SheetView);

impl SettingsView {
    /// The search field the sheet uses: magnifier, the typed text and caret, and the clear mark at its end.
    pub(super) fn field<T: Host + gpui::EntityInputHandler>(
        view: gpui::Entity<T>,
        ink: &Ink,
        corner: f32,
        typeface: &Typeface,
        s: Scale,
    ) -> Div {
        div()
            .h(px(s.of(g::FIELD_H)))
            .flex()
            .items_center()
            .gap(px(s.of(8.)))
            .px(px(s.of(g::GAP)))
            .border_1()
            .border_color(ink.accent_frame)
            .rounded(px(corner))
            .bg(ink.background)
            .text_size(typeface.body)
            .text_color(ink.foreground)
            .child(mark("search", s.of(14.), ink.muted))
            .child(Field::mount(view, typeface, true))
            .child(mark("x", s.of(12.), ink.muted))
    }
}

impl SheetView {
    pub(super) fn key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_editor();
        match self.editor.key(event, cx) {
            KeyUse::Text if !type_key(self, event, window, cx) => return,
            KeyUse::Text => {}
            KeyUse::Handled => self.edited(cx),
            KeyUse::Command => {
                self.sheet.key(&event.keystroke.key, "");
                self.flush(cx);
            }
        }
        cx.stop_propagation();
    }

    pub(super) fn flush(&mut self, cx: &mut Context<Self>) {
        for request in std::mem::take(&mut self.sheet.requests) {
            if request == Request::Close {
                self.editor.retire();
            }
            cx.emit(request);
        }
        cx.notify();
    }
}

impl SettingsView {
    pub(super) fn sync_plugin_input(&self, cx: &mut Context<Self>) {
        if let Some(plugin) = &self.plugins {
            let enabled = !self.editor.retired
                && self.panel.section == "plugins"
                && self.panel.side == super::Side::Pane
                && self.panel.query.is_none();
            plugin.update(cx, |plugin, cx| {
                if plugin.mounted_input_focus(self.focus.clone(), enabled, self.typeface.clone()) {
                    cx.notify();
                }
            });
        }
    }
}
