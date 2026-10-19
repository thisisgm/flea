//! The field's text input: typed characters and the input method's composing text, in UTF-16 as the platform counts.
use super::view::PathView;
use gpui::{point, Bounds, Context, EntityInputHandler, Pixels, Point, UTF16Selection, Window};
use std::ops::Range;

// Sample input: ("a\u{e9}b", 2) is 3, the byte where the second UTF-16 unit ends.
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

impl EntityInputHandler for PathView {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.entry.range_from_utf16(&range);
        adjusted.replace(self.entry.range_to_utf16(&range));
        self.entry.text.get(range).map(str::to_owned)
    }

    fn selected_text_range(&mut self, _: bool, _: &mut Window, _: &mut Context<Self>) -> Option<UTF16Selection> {
        self.entry.editing.then(|| UTF16Selection {
            range: self.entry.range_to_utf16(&self.entry.selected),
            reversed: self.entry.reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.entry.marked.as_ref().map(|range| self.entry.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.entry.marked = None;
        cx.notify();
    }

    fn replace_text_in_range(&mut self, range: Option<Range<usize>>, text: &str, _: &mut Window, cx: &mut Context<Self>) {
        // A held Enter stands for the line as it was.
        if !self.entry.editing || self.jump.enter_waiting {
            return;
        }
        let range = range.map(|range| self.entry.range_from_utf16(&range));
        self.entry.replace(range, text);
        self.edited(cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.entry.editing || self.jump.enter_waiting {
            return;
        }
        let range = range.map(|range| self.entry.range_from_utf16(&range));
        // The selection inside the composing text counts UTF-16 units of that text alone.
        let selected = selected.map(|inner| bytes_at(text, inner.start)..bytes_at(text, inner.end));
        self.entry.replace_and_mark(range, text, selected);
        self.edited(cx);
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.entry.range_from_utf16(&range);
        let origin = self.paint.borrow().field_origin;
        let line = self.line.borrow();
        let left = origin - line.scroll;
        Some(Bounds::from_corners(
            point(left + line.x_for(range.start), bounds.top()),
            point(left + line.x_for(range.end), bounds.bottom()),
        ))
    }

    fn character_index_for_point(&mut self, at: Point<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        let origin = self.paint.borrow().field_origin;
        let line = self.line.borrow();
        let index = line.index_for(at.x - origin + line.scroll);
        drop(line);
        Some(self.entry.offset_to_utf16(index))
    }
}
