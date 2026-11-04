//! The mounted destination's platform text, through Entry's shared UTF-16 and composition contract.
use super::host::OutputView;
use crate::path::field::{ime_bounds, ime_index};
use gpui::{Bounds, Context, EntityInputHandler, Pixels, Point, UTF16Selection, Window};
use std::ops::Range;

impl EntityInputHandler for OutputView {
    fn accepts_text_input(&self, _: &mut Window, _: &mut Context<Self>) -> bool {
        self.editor.entry.composes()
    }
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        self.ime_field(window, cx)
            .then(|| self.editor.entry.ime_text(&range, adjusted))
            .flatten()
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        self.ime_field(window, cx)
            .then(|| self.editor.entry.ime_selection())
    }

    fn marked_text_range(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.ime_field(window, cx)
            .then(|| self.editor.entry.ime_marked())
            .flatten()
    }

    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.ime_field(window, cx) {
            self.editor.entry.ime_unmark();
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
        if self.ime_field(window, cx)
            && self.fits_ime(&range, text)
            && self.editor.entry.ime_replace(range, text)
        {
            self.edited_field(cx);
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
        if self.ime_field(window, cx)
            && self.fits_ime(&range, text)
            && self.editor.entry.ime_mark(range, text, selected)
        {
            self.edited_field(cx);
        }
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        if !self.ime_field(window, cx) {
            return None;
        }
        let drawn = self.editor.drawn.borrow();
        Some(ime_bounds(
            &drawn.line,
            &self.editor.entry,
            &range,
            drawn.bounds?,
        ))
    }

    fn character_index_for_point(
        &mut self,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        if !self.ime_field(window, cx) {
            return None;
        }
        let drawn = self.editor.drawn.borrow();
        let left = drawn.bounds.map_or(gpui::px(0.), |bounds| bounds.left());
        Some(ime_index(&drawn.line, &self.editor.entry, at.x - left))
    }
}

impl OutputView {
    fn fits_ime(&self, range: &Option<Range<usize>>, text: &str) -> bool {
        let entry = &self.editor.entry;
        let bytes = range
            .as_ref()
            .map(|r| entry.ime_range(r))
            .or_else(|| entry.marked.clone())
            .unwrap_or_else(|| entry.selected.clone());
        entry.text.len() - bytes.len() + text.len() <= 4096
    }
}
