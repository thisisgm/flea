//! The rename editor's text input: typed characters and the input method's composing text, in UTF-16 as the platform counts.
use super::{Edit, Shell};
use gpui::{point, Bounds, EntityInputHandler, Pixels, Point, UTF16Selection, Window};
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

impl Shell {
    fn editing(&mut self) -> Option<&mut Edit> {
        self.rename
            .edit
            .as_mut()
            .filter(|edit| edit.entry.editing && edit.pending.is_none())
    }
}

impl EntityInputHandler for Shell {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut gpui::Context<Self>,
    ) -> Option<String> {
        let entry = &self.editing()?.entry;
        let range = entry.range_from_utf16(&range);
        adjusted.replace(entry.range_to_utf16(&range));
        entry.text.get(range).map(str::to_owned)
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut gpui::Context<Self>,
    ) -> Option<UTF16Selection> {
        let entry = &self.editing()?.entry;
        Some(UTF16Selection {
            range: entry.range_to_utf16(&entry.selected),
            reversed: entry.reversed,
        })
    }

    fn marked_text_range(
        &self,
        _: &mut Window,
        _: &mut gpui::Context<Self>,
    ) -> Option<Range<usize>> {
        let entry = &self.rename.edit.as_ref()?.entry;
        entry
            .marked
            .as_ref()
            .map(|range| entry.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) {
        if let Some(edit) = self.editing() {
            edit.entry.marked = None;
            cx.notify();
        }
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(edit) = self.editing() else {
            return;
        };
        let text: String = text.chars().filter(|ch| !ch.is_control()).collect();
        let range = range.map(|range| edit.entry.range_from_utf16(&range));
        edit.entry.replace(range, &text);
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(edit) = self.editing() else {
            return;
        };
        let range = range.map(|range| edit.entry.range_from_utf16(&range));
        // The selection inside the composing text counts UTF-16 units of that text alone.
        let selected = selected.map(|inner| bytes_at(text, inner.start)..bytes_at(text, inner.end));
        edit.entry.replace_and_mark(range, text, selected);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut gpui::Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let edit = self.rename.edit.as_ref()?;
        let range = edit.entry.range_from_utf16(&range);
        let left = edit.line.origin();
        Some(Bounds::from_corners(
            point(left + edit.line.line.x_for(range.start), bounds.top()),
            point(left + edit.line.line.x_for(range.end), bounds.bottom()),
        ))
    }

    fn character_index_for_point(
        &mut self,
        at: Point<Pixels>,
        _: &mut Window,
        _: &mut gpui::Context<Self>,
    ) -> Option<usize> {
        let edit = self.rename.edit.as_ref()?;
        let index = edit.line.line.index_for(at.x - edit.line.origin());
        Some(edit.entry.offset_to_utf16(index))
    }
}
