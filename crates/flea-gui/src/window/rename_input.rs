//! Inline names and operation cards forward platform callbacks to their currently painted entry session.
use super::Shell;
use crate::path::{
    entry::Entry,
    field::{ime_bounds, ime_box, ime_index},
};
use gpui::{Bounds, Context, EntityInputHandler, Pixels, Point, UTF16Selection, Window};
use std::ops::Range;

impl Shell {
    fn ime_entry(&self, window: &Window) -> Option<&Entry> {
        if self.dialog_showing() {
            return (self.dialog_active(window)
                && self
                    .rename
                    .ime
                    .drawn
                    .borrow()
                    .line
                    .accepts(&self.rename.ime.entry))
            .then_some(&self.rename.ime.entry);
        }
        let edit = self.rename.edit.as_ref()?;
        (edit.pending.is_none()
            && self.focus.is_focused(window)
            && edit.line.line.accepts(&edit.entry))
        .then_some(&edit.entry)
    }
    fn ime_entry_mut(&mut self) -> &mut Entry {
        if self.dialog_showing() {
            &mut self.rename.ime.entry
        } else {
            &mut self.rename.edit.as_mut().expect("active inline name").entry
        }
    }
    fn ime_changed(&mut self, cx: &mut Context<Self>) {
        if self.dialog_showing() {
            self.dialog_edited(cx);
        } else {
            cx.notify();
        }
    }
}

impl EntityInputHandler for Shell {
    fn accepts_text_input(&self, _: &mut Window, _: &mut Context<Self>) -> bool {
        !self.dialog_showing() || self.rename.ime.entry.composes()
    }
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        self.ime_entry(window)?.ime_text(&range, adjusted)
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(self.ime_entry(window)?.ime_selection())
    }
    fn marked_text_range(
        &self,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.ime_entry(window)?.ime_marked()
    }
    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.ime_entry(window).is_some() {
            self.ime_entry_mut().ime_unmark();
            self.ime_changed(cx);
        }
    }
    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.ime_entry(window).is_some() && self.ime_entry_mut().ime_replace(range, text) {
            self.ime_changed(cx);
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
        if self.ime_entry(window).is_some() && self.ime_entry_mut().ime_mark(range, text, selected)
        {
            self.ime_changed(cx);
        }
    }
    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let entry = self.ime_entry(window)?;
        if self.dialog_showing() {
            let drawn = self.rename.ime.drawn.borrow();
            return Some(crate::g2::file_ops::field_view::bounds(
                self,
                &range,
                drawn.bounds?,
            ));
        }
        let edit = self.rename.edit.as_ref()?;
        let text = ime_box(edit.line.frame?, edit.line.origin() + edit.line.line.scroll);
        Some(ime_bounds(&edit.line.line, entry, &range, text))
    }
    fn character_index_for_point(
        &mut self,
        at: Point<Pixels>,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let entry = self.ime_entry(window)?;
        if self.dialog_showing() {
            let drawn = self.rename.ime.drawn.borrow();
            let byte = crate::g2::file_ops::field_view::index(self, at.x - drawn.bounds?.left());
            return Some(entry.offset_to_utf16(byte));
        }
        let edit = self.rename.edit.as_ref()?;
        Some(ime_index(
            &edit.line.line,
            entry,
            at.x - edit.line.origin() - edit.line.line.scroll,
        ))
    }
}
