//! Text and composition follow the shared path-field editor; password queries never return its bytes.
use super::{dialog::Focus, view::NetworkView};
use gpui::{Bounds, Context, EntityInputHandler, Pixels, Point, UTF16Selection, Window};
use std::ops::Range;

impl EntityInputHandler for NetworkView {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        if self.model.focus == Focus::Password {
            return None;
        }
        let range = self.input.range_from_utf16(&range);
        adjusted.replace(self.input.range_to_utf16(&range));
        self.input.text.get(range).map(str::to_owned)
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        if self.model.focus == Focus::Password {
            return Some(UTF16Selection {
                range: self.password_selection.clone(),
                reversed: self.password_reversed,
            });
        }
        self.input.editing.then(|| UTF16Selection {
            range: self.input.range_to_utf16(&self.input.selected),
            reversed: self.input.reversed,
        })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        if self.model.focus == Focus::Password {
            return self.password_marked.clone();
        }
        self.input
            .marked
            .as_ref()
            .map(|range| self.input.range_to_utf16(range))
    }
    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.password_marked = None;
        self.input.marked = None;
        self.changed(cx);
    }
    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.model.opened
            || (self.model.busy() || self.model.committed)
            || text.chars().any(char::is_control)
        {
            return;
        }
        if self.model.focus == Focus::Password {
            let range = range
                .or(self.password_marked.take())
                .unwrap_or_else(|| self.password_selection.clone());
            self.password_selection =
                super::secret_edit::replace(&mut self.model.password, range, text);
            self.password_reversed = false;
            self.changed(cx);
            return;
        }
        if !self.input.editing {
            return;
        }
        let range = range.map(|r| self.input.range_from_utf16(&r));
        self.input.replace(range, text);
        self.edited(cx);
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.model.opened
            || self.model.busy()
            || self.model.committed
            || text.chars().any(char::is_control)
        {
            return;
        }
        if self.model.focus == Focus::Password {
            let start = range.as_ref().map_or_else(
                || {
                    self.password_marked
                        .as_ref()
                        .unwrap_or(&self.password_selection)
                        .start
                },
                |range| range.start,
            );
            self.replace_text_in_range(range, text, window, cx);
            let units = text.encode_utf16().count();
            self.password_marked = Some(start..start + units);
            if let Some(range) = selected {
                self.password_selection = start + range.start.min(units)
                    ..start + range.end.min(units).max(range.start.min(units));
            }
            self.password_reversed = false;
            self.changed(cx);
            return;
        }
        if !self.model.opened
            || (self.model.busy() || self.model.committed)
            || !self.input.editing
            || text.chars().any(char::is_control)
        {
            return;
        }
        let range = range.map(|r| self.input.range_from_utf16(&r));
        let selected = selected.map(|r| bytes(text, r.start)..bytes(text, r.end));
        self.input.replace_and_mark(range, text, selected);
        self.edited(cx);
    }
    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        if self.input.editing {
            let drawn = self.drawn.borrow();
            if let Some(text) = drawn
                .text
                .iter()
                .find(|text| text.field == Some(self.model.focus))
            {
                let range = self.input.range_from_utf16(&range);
                let x = gpui::px(drawn.origin.0 + text.rect.x);
                return Some(Bounds::from_corners(
                    gpui::point(x + text.run.line().x_for_index(range.start), bounds.top()),
                    gpui::point(x + text.run.line().x_for_index(range.end), bounds.bottom()),
                ));
            }
        }
        Some(bounds)
    }
    fn character_index_for_point(
        &mut self,
        at: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.point_units(at))
    }
}

fn bytes(text: &str, units: usize) -> usize {
    let mut count = 0;
    for (index, c) in text.char_indices() {
        if count >= units {
            return index;
        }
        count += c.len_utf16();
    }
    text.len()
}
