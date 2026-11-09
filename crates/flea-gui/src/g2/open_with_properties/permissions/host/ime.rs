//! The mounted octal field's platform text and shared painter, refusing any callback outside its live session.
use super::View;
use crate::{
    g2::file_ops::{
        editor::Drawn,
        field_view::{self, FieldView},
        palette::CardPalette,
    },
    path::{
        entry::Entry,
        field::{ime_bounds, ime_index},
    },
};
use gpui::{
    App, Bounds, Context, EntityInputHandler, FocusHandle, MouseDownEvent, MouseMoveEvent, Pixels,
    Point, UTF16Selection, Window,
};
use std::{cell::RefCell, ops::Range};

impl View {
    fn ime_octal(&self, window: &Window) -> bool {
        self.editing_octal(window) && self.drawn.borrow().line.accepts(&self.editor)
    }
}

impl EntityInputHandler for View {
    fn accepts_text_input(&self, _: &mut Window, _: &mut Context<Self>) -> bool {
        self.editor.composes()
    }
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        self.ime_octal(window)
            .then(|| self.editor.ime_text(&range, adjusted))
            .flatten()
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        self.ime_octal(window).then(|| self.editor.ime_selection())
    }

    fn marked_text_range(
        &self,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.ime_octal(window)
            .then(|| self.editor.ime_marked())
            .flatten()
    }

    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.ime_octal(window) && self.editor.ime_unmark() {
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
        if self.ime_octal(window) && self.editor.ime_replace(range, text) {
            self.edited(cx);
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.ime_octal(window) && self.editor.ime_mark(range, text, selected) {
            self.edited(cx);
        }
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        if !self.ime_octal(window) {
            return None;
        }
        let drawn = self.drawn.borrow();
        Some(ime_bounds(&drawn.line, &self.editor, &range, drawn.bounds?))
    }

    fn character_index_for_point(
        &mut self,
        at: Point<Pixels>,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        if !self.ime_octal(window) {
            return None;
        }
        let drawn = self.drawn.borrow();
        let left = drawn.bounds?.left();
        Some(ime_index(&drawn.line, &self.editor, at.x - left))
    }
}

impl FieldView for View {
    fn field_entry(&self) -> &Entry {
        &self.editor
    }
    fn field_drawn(&self) -> &RefCell<Drawn> {
        &self.drawn
    }
    fn field_active(&self, window: &Window, _: &App) -> bool {
        self.editing_octal(window)
    }
    fn field_palette(&self) -> CardPalette {
        self.palette
    }
    fn field_focus(&self) -> Option<FocusHandle> {
        Some(self.focus.clone())
    }
    fn field_press(&mut self, event: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let left = self
            .drawn
            .borrow()
            .bounds
            .map_or(gpui::px(0.), |b| b.left());
        let at = field_view::index(self, event.position.x - left);
        if event.click_count > 1 {
            self.editor.select_all();
        } else if event.modifiers.shift {
            self.editor.select_to(at);
        } else {
            self.editor.move_to(at);
        }
        cx.stop_propagation();
        cx.notify();
    }
    fn field_drag(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.pressed_button != Some(gpui::MouseButton::Left) || !self.editing_octal(window) {
            return;
        }
        let left = self
            .drawn
            .borrow()
            .bounds
            .map_or(gpui::px(0.), |b| b.left());
        let at = field_view::index(self, event.position.x - left);
        self.editor.select_to(at);
        cx.notify();
    }
}
