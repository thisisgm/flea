//! The rail editor reuses the path field's Unicode selection model and native input contract.
use super::*;
use gpui::{Bounds, EntityInputHandler, Pixels, Point, UTF16Selection};
use std::ops::Range;

fn bytes_at(text: &str, units: usize) -> usize {
    let mut count = 0;
    for (at, ch) in text.char_indices() {
        if count >= units {
            return at;
        }
        count += ch.len_utf16();
    }
    text.len()
}

impl EntityInputHandler for Producer {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let model = self.model.borrow();
        model.renaming?;
        let range = model.editor.range_from_utf16(&range);
        adjusted.replace(model.editor.range_to_utf16(&range));
        model.editor.text.get(range).map(str::to_owned)
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let model = self.model.borrow();
        model.renaming.map(|_| UTF16Selection {
            range: model.editor.range_to_utf16(&model.editor.selected),
            reversed: model.editor.reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        let model = self.model.borrow();
        model
            .editor
            .marked
            .as_ref()
            .map(|range| model.editor.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.model.borrow_mut().editor.marked = None;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut model = self.model.borrow_mut();
        if model.renaming.is_none() {
            return;
        }
        let range = range.map(|range| model.editor.range_from_utf16(&range));
        model.editor.replace(
            range,
            &text
                .chars()
                .filter(|ch| !ch.is_control())
                .collect::<String>(),
        );
        drop(model);
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut model = self.model.borrow_mut();
        if model.renaming.is_none() {
            return;
        }
        let range = range.map(|range| model.editor.range_from_utf16(&range));
        let text: String = text.chars().filter(|ch| !ch.is_control()).collect();
        let selected =
            selected.map(|range| bytes_at(&text, range.start)..bytes_at(&text, range.end));
        model.editor.replace_and_mark(range, &text, selected);
        drop(model);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let model = self.model.borrow();
        let laid = self.laid.borrow();
        let rect = laid.row(model.renaming?)?.editor?;
        let range = model.editor.range_from_utf16(&range);
        let left = rect.x + 4. - model.editor_scroll;
        Some(Bounds::from_corners(
            gpui::point(
                gpui::px(left + model.editor_x(range.start)),
                gpui::px(rect.y),
            ),
            gpui::point(
                gpui::px(left + model.editor_x(range.end)),
                gpui::px(rect.y + rect.height),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let model = self.model.borrow();
        let laid = self.laid.borrow();
        let rect = laid.row(model.renaming?)?.editor?;
        let byte = model.editor_index(f32::from(point.x) - rect.x - 4. + model.editor_scroll);
        Some(model.editor.offset_to_utf16(byte))
    }
}
