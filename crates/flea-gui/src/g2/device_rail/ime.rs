//! The unlock input uses shared entry metadata and masked geometry without putting secrets in the text engine.
use super::{
    private_entry,
    unlock::{Part, Phase},
    view::UnlockView,
};
use crate::{
    g2::file_ops::{editor::Drawn, field_view::FieldView, palette::CardPalette},
    path::{entry::Entry, field::ime_index},
};
use gpui::{
    App, Bounds, Context, EntityInputHandler, FocusHandle, KeyDownEvent, MouseDownEvent,
    MouseMoveEvent, Pixels, Point, UTF16Selection, Window,
};
use std::{cell::RefCell, ops::Range};

impl UnlockView {
    pub(super) fn sync_input(&mut self) {
        let active = self.host.card.as_ref().is_some_and(|c| {
            c.opened && c.focus == Some(Part::Field) && c.phase != Phase::Unlocking
        });
        if let Some(card) = &mut self.host.card {
            if active && (!self.input_active || !card.field.entry.editing) {
                card.field.begin();
                self.drawn.borrow_mut().line = Default::default();
            } else if !active {
                card.field.entry.ime_unmark();
                card.field.entry.editing = false;
                self.drawn.borrow_mut().line.registered = None;
            }
        }
        self.input_active = active;
    }
    fn active(&self, window: &Window) -> bool {
        self.input_active
            && self.focus_handle().is_focused(window)
            && self.host.card.as_ref().is_some_and(|c| {
                c.opened
                    && c.focus == Some(Part::Field)
                    && c.phase != Phase::Unlocking
                    && self.drawn.borrow().line.accepts(&c.field.entry)
            })
    }
    fn replace(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        marked: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(card) = &mut self.host.card else {
            return;
        };
        let field = &mut card.field;
        if private_entry::replace(&mut field.entry, &mut field.text, range, text, marked) {
            field.held_mask = 0;
            card.edited();
            self.changed_input(cx);
        }
    }
    fn changed_input(&mut self, cx: &mut Context<Self>) {
        self.refresh(cx);
    }
    pub(super) fn private_key(
        &mut self,
        event: &KeyDownEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.input_active || !self.focus_handle().is_focused(window) {
            return false;
        }
        let Some(card) =
            self.host.card.as_mut().filter(|c| {
                c.opened && c.phase != Phase::Unlocking && c.focus == Some(Part::Field)
            })
        else {
            return false;
        };
        let field = &mut card.field;
        let key = event.keystroke.key.as_str();
        let held = event.keystroke.modifiers;
        let command = held.control || held.platform;
        let mut handled = true;
        if !field.entry.ime_key(key) {
            let entry = &mut field.entry;
            let at = entry.cursor();
            match key {
                "left" | "right" | "home" | "end" if !command && !held.alt => {
                    let to = match key {
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
                "backspace" | "delete" if !command && !held.alt => {
                    private_entry::erase(entry, &mut field.text, key == "delete")
                }
                "a" if command => entry.select_all(),
                "c" | "x" if command => {}
                _ if crate::window::input::paste_key(event) => {
                    if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                        self.replace(None, &crate::window::input::clean_paste(&text), false, cx);
                    }
                }
                "enter" | "return" | "kp_enter" | "escape" | "tab" | "backtab" => handled = false,
                _ if command || held.alt => handled = false,
                _ if event.keystroke.key_char.is_some() => return true,
                _ => {}
            }
        }
        if handled {
            cx.stop_propagation();
            self.changed_input(cx);
        }
        handled
    }
}

impl FieldView for UnlockView {
    fn field_entry(&self) -> &Entry {
        &self
            .host
            .card
            .as_ref()
            .expect("mounted unlock card")
            .field
            .entry
    }
    fn field_drawn(&self) -> &RefCell<Drawn> {
        &self.drawn
    }
    fn field_active(&self, window: &Window, _: &App) -> bool {
        self.input_active && self.focus_handle().is_focused(window)
    }
    fn field_palette(&self) -> CardPalette {
        self.input_palette.expect("mounted unlock palette")
    }
    fn field_focus(&self) -> Option<FocusHandle> {
        Some(self.focus_handle().clone())
    }
    fn field_held(&self) -> usize {
        self.host
            .card
            .as_ref()
            .map_or(0, |card| card.field.held_mask)
    }
    fn field_press(&mut self, event: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(card) = &mut self.host.card {
            card.focus = Some(Part::Field);
        }
        self.sync_input();
        let drawn = self.drawn.borrow();
        let left = drawn.bounds.map_or(gpui::px(0.), |b| b.left());
        let entry = &self.host.card.as_ref().unwrap().field.entry;
        let at = entry.from_display(
            drawn
                .line
                .index_for(event.position.x - left + drawn.line.scroll),
        );
        drop(drawn);
        let entry = &mut self.host.card.as_mut().unwrap().field.entry;
        if event.click_count > 1 {
            entry.select_all();
        } else if event.modifiers.shift {
            entry.select_to(at);
        } else {
            entry.move_to(at);
        }
        cx.stop_propagation();
        cx.notify();
    }
    fn field_drag(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if event.pressed_button != Some(gpui::MouseButton::Left) {
            return;
        }
        let drawn = self.drawn.borrow();
        let left = drawn.bounds.map_or(gpui::px(0.), |b| b.left());
        let entry = &self.host.card.as_ref().unwrap().field.entry;
        let at = entry.from_display(
            drawn
                .line
                .index_for(event.position.x - left + drawn.line.scroll),
        );
        drop(drawn);
        self.host.card.as_mut().unwrap().field.entry.select_to(at);
        cx.notify();
    }
}

impl EntityInputHandler for UnlockView {
    // The only field is the passphrase, so the input method never serves this view.
    fn accepts_text_input(&self, _: &mut Window, _: &mut Context<Self>) -> bool {
        false
    }
    fn text_for_range(
        &mut self,
        _: Range<usize>,
        _: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        None
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        self.active(window)
            .then(|| self.field_entry().ime_selection())
    }
    fn marked_text_range(
        &self,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.active(window)
            .then(|| self.field_entry().ime_marked())
            .flatten()
    }
    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.active(window) {
            self.host.card.as_mut().unwrap().field.entry.ime_unmark();
            cx.notify();
        }
    }
    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.active(window) {
            self.replace(range, text, false, cx);
        }
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.active(window) {
            self.replace(range, text, true, cx);
        }
    }
    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        if !self.active(window) {
            return None;
        }
        let drawn = self.drawn.borrow();
        Some(crate::g2::file_ops::field_view::bounds(
            self,
            &range,
            drawn.bounds?,
        ))
    }
    fn character_index_for_point(
        &mut self,
        at: Point<Pixels>,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        if !self.active(window) {
            return None;
        }
        let drawn = self.drawn.borrow();
        Some(ime_index(
            &drawn.line,
            self.field_entry(),
            at.x - drawn.bounds?.left(),
        ))
    }
}
