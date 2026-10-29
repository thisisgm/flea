//! Native chooser callbacks share Entry's UTF-16 contract and refuse stale sessions.
use super::{filename::Filename, picker::Picker, text::EntryHost};
use crate::path::field::{ime_bounds, ime_index};
use gpui::{Bounds, Context, EntityInputHandler, Pixels, Point, UTF16Selection, Window};
use std::ops::Range;

macro_rules! input_handler {
    ($host:ty) => {
        impl EntityInputHandler for $host {
            fn text_for_range(
                &mut self,
                range: Range<usize>,
                adjusted: &mut Option<Range<usize>>,
                window: &mut Window,
                cx: &mut Context<Self>,
            ) -> Option<String> {
                self.accepts(window, cx)
                    .then(|| self.entry().ime_text(&range, adjusted))
                    .flatten()
            }
            fn selected_text_range(
                &mut self,
                _: bool,
                window: &mut Window,
                cx: &mut Context<Self>,
            ) -> Option<UTF16Selection> {
                self.accepts(window, cx)
                    .then(|| self.entry().ime_selection())
            }
            fn marked_text_range(
                &self,
                window: &mut Window,
                cx: &mut Context<Self>,
            ) -> Option<Range<usize>> {
                self.accepts(window, cx)
                    .then(|| self.entry().ime_marked())
                    .flatten()
            }
            fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
                if self.accepts(window, cx) && self.entry_mut().ime_unmark() {
                    self.changed(cx);
                }
            }
            fn replace_text_in_range(
                &mut self,
                range: Option<Range<usize>>,
                text: &str,
                window: &mut Window,
                cx: &mut Context<Self>,
            ) {
                if self.accepts(window, cx) && self.entry_mut().ime_replace(range, text) {
                    self.changed(cx);
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
                if self.accepts(window, cx) && self.entry_mut().ime_mark(range, text, selected) {
                    self.changed(cx);
                }
            }
            fn bounds_for_range(
                &mut self,
                range: Range<usize>,
                _: Bounds<Pixels>,
                window: &mut Window,
                cx: &mut Context<Self>,
            ) -> Option<Bounds<Pixels>> {
                if !self.accepts(window, cx) {
                    return None;
                }
                let drawn = self.drawn().borrow();
                Some(ime_bounds(&drawn.line, self.entry(), &range, drawn.bounds?))
            }
            fn character_index_for_point(
                &mut self,
                point: Point<Pixels>,
                window: &mut Window,
                cx: &mut Context<Self>,
            ) -> Option<usize> {
                if !self.accepts(window, cx) {
                    return None;
                }
                let drawn = self.drawn().borrow();
                Some(ime_index(
                    &drawn.line,
                    self.entry(),
                    point.x - drawn.bounds?.left(),
                ))
            }
        }
    };
}

input_handler!(Filename);
input_handler!(Picker);
