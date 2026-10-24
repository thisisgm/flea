//! The mounted destination's platform text and IME ranges use UTF-16 at the boundary.
use super::host::Overlay;
use gpui::{point, Bounds, Context, EntityInputHandler, Pixels, Point, UTF16Selection, Window};
use std::ops::Range;

impl EntityInputHandler for Overlay {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        if !self.editing_copy(window, cx) {
            return None;
        }
        let entry = &self.editor.entry;
        let range = entry.range_from_utf16(&range);
        adjusted.replace(entry.range_to_utf16(&range));
        entry.text.get(range).map(str::to_owned)
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        self.editing_copy(window, cx).then(|| UTF16Selection {
            range: self
                .editor
                .entry
                .range_to_utf16(&self.editor.entry.selected),
            reversed: self.editor.entry.reversed,
        })
    }

    fn marked_text_range(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.editing_copy(window, cx)
            .then(|| {
                self.editor
                    .entry
                    .marked
                    .as_ref()
                    .map(|r| self.editor.entry.range_to_utf16(r))
            })
            .flatten()
    }

    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing_copy(window, cx) {
            self.editor.entry.marked = None;
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
        if !self.editing_copy(window, cx) || text.chars().any(char::is_control) {
            return;
        }
        let entry = &mut self.editor.entry;
        let range = range.map(|r| entry.range_from_utf16(&r));
        entry.replace(range, text);
        self.edited_copy(cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.editing_copy(window, cx) || text.chars().any(char::is_control) {
            return;
        }
        let entry = &mut self.editor.entry;
        let range = range.map(|r| entry.range_from_utf16(&r));
        let selected = selected.map(|r| bytes_at(text, r.start)..bytes_at(text, r.end));
        entry.replace_and_mark(range, text, selected);
        self.edited_copy(cx);
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        if !self.editing_copy(window, cx) {
            return None;
        }
        let drawn = self.editor.drawn.borrow();
        let bounds = drawn.bounds?;
        let range = self.editor.entry.range_from_utf16(&range);
        let left = bounds.left() - drawn.line.scroll;
        Some(Bounds::from_corners(
            point(left + drawn.line.x_for(range.start), bounds.top()),
            point(left + drawn.line.x_for(range.end), bounds.bottom()),
        ))
    }

    fn character_index_for_point(
        &mut self,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        self.editing_copy(window, cx)
            .then(|| self.editor.entry.offset_to_utf16(self.copy_index(at.x)))
    }
}

fn bytes_at(text: &str, units: usize) -> usize {
    let mut count = 0;
    for (at, next) in text.char_indices() {
        if count >= units {
            return at;
        }
        count += next.len_utf16();
    }
    text.len()
}
