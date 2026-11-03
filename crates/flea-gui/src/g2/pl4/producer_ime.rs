//! Rail callbacks use the shared UTF-16 contract and only the session drawn for this focus.
use super::*;
use gpui::{Bounds, EntityInputHandler, Pixels, Point, UTF16Selection};
use std::ops::Range;

impl Producer {
    pub(super) fn ime_live(&self, window: &Window) -> bool {
        let model = self.model.borrow();
        model.focused
            && model
                .renaming
                .is_some_and(|index| self.entry(index).is_some())
            && model.editor.session().is_some()
            && self.ime_session == model.editor.session()
            && self
                .ime_focus
                .as_ref()
                .is_some_and(|focus| focus.is_focused(window))
    }
}

impl EntityInputHandler for Producer {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        self.ime_live(window)
            .then(|| self.model.borrow().editor.ime_text(&range, adjusted))
            .flatten()
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        self.ime_live(window)
            .then(|| self.model.borrow().editor.ime_selection())
    }
    fn marked_text_range(
        &self,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.ime_live(window)
            .then(|| self.model.borrow().editor.ime_marked())
            .flatten()
    }
    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.ime_live(window) && self.model.borrow_mut().editor.ime_unmark() {
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
        if self.ime_live(window) && self.model.borrow_mut().editor.ime_replace(range, text) {
            cx.notify();
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
        if self.ime_live(window)
            && self
                .model
                .borrow_mut()
                .editor
                .ime_mark(range, text, selected)
        {
            cx.notify();
        }
    }
    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        if !self.ime_live(window) {
            return None;
        }
        let model = self.model.borrow();
        let laid = self.laid.borrow();
        let rect = laid.row(model.renaming?)?.editor?;
        let range = model.editor.ime_range(&range);
        let left = rect.x + 4. - model.editor_scroll;
        let (from, to) = (model.editor_x(range.start), model.editor_x(range.end));
        Some(Bounds::from_corners(
            gpui::point(gpui::px(left + from.min(to)), gpui::px(rect.y)),
            gpui::point(
                gpui::px(left + from.max(to)),
                gpui::px(rect.y + rect.height),
            ),
        ))
    }
    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        if !self.ime_live(window) {
            return None;
        }
        let model = self.model.borrow();
        let laid = self.laid.borrow();
        let rect = laid.row(model.renaming?)?.editor?;
        let byte = model.editor_index(f32::from(point.x) - rect.x - 4. + model.editor_scroll);
        Some(model.editor.offset_to_utf16(byte))
    }
}
