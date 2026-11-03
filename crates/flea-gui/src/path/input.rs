//! The mounted destination's platform text, through Entry's shared UTF-16 and composition contract.
use super::view::PathView;
use crate::path::field::{ime_bounds, ime_box, ime_index};
use gpui::{Bounds, Context, EntityInputHandler, Pixels, Point, UTF16Selection, Window};
use std::ops::Range;

impl EntityInputHandler for PathView {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        self.ime_field(window, cx)
            .then(|| self.entry.ime_text(&range, adjusted))
            .flatten()
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        self.ime_field(window, cx)
            .then(|| self.entry.ime_selection())
    }

    fn marked_text_range(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.ime_field(window, cx)
            .then(|| self.entry.ime_marked())
            .flatten()
    }

    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.ime_field(window, cx) {
            self.entry.ime_unmark();
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
        if self.ime_field(window, cx) && self.entry.ime_replace(range, text) {
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
        if self.ime_field(window, cx) && self.entry.ime_mark(range, text, selected) {
            self.edited(cx);
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
        let paint = self.paint.borrow();
        let field = ime_box(paint.frame?, paint.field_origin);
        Some(ime_bounds(&self.line.borrow(), &self.entry, &range, field))
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
        Some(ime_index(
            &self.line.borrow(),
            &self.entry,
            at.x - self.paint.borrow().field_origin,
        ))
    }
}

impl PathView {
    fn ime_field(&self, window: &Window, _: &gpui::App) -> bool {
        self.holds_keyboard(window)
            && !self.jump.enter_waiting
            && self.line.borrow().accepts(&self.entry)
    }
}

#[cfg(test)]
mod i50_tests {
    use super::*;
    #[gpui::test]
    fn i50_dialog_go_to_composition_and_closed_owner(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, cx| {
            PathView::new(
                std::sync::Arc::new(crate::test_catalogs::english()),
                Box::new(crate::path::suggestions::Unranked),
                cx,
            )
        });
        cx.simulate_resize(gpui::size(gpui::px(600.), gpui::px(27.)));
        view.update_in(cx, |v, w, cx| v.open(w, cx));
        cx.run_until_parked();
        cx.update(|w, cx| {
            w.refresh();
            w.draw(cx).clear(cx);
        });
        view.update_in(cx, |v, w, cx| {
            v.replace_and_mark_text_in_range(None, "/猫", Some(2..2), w, cx);
            assert_eq!(v.marked_text_range(w, cx), Some(0..2));
        });
        cx.simulate_keystrokes("enter");
        assert!(view.read_with(cx, |v, _| v.entry.editing));
        cx.simulate_keystrokes("escape");
        assert!(view.read_with(cx, |v, _| v.entry.editing && v.entry.marked.is_none()));
        view.update_in(cx, |v, w, cx| {
            v.close(cx);
            v.replace_text_in_range(None, "late", w, cx);
            assert!(v.selected_text_range(false, w, cx).is_none());
            v.open(w, cx);
            v.replace_and_mark_text_in_range(None, "late", None, w, cx);
            assert!(v.entry.marked.is_none());
        });
    }
}

#[cfg(test)]
#[path = "i50_ime_tests.rs"]
mod path_ime_tests;
