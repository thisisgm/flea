//! Platform callbacks belong to the active field session, and private input exposes only masked geometry.
use super::{dialog::Focus, view::NetworkView};
use gpui::{Bounds, Context, EntityInputHandler, Pixels, Point, UTF16Selection, Window};
use std::ops::Range;

impl NetworkView {
    pub(super) fn ime_active(&self, window: &Window) -> bool {
        self.model.opened
            && !self.model.busy()
            && !self.model.committed
            && self.focus.is_focused(window)
            && self.input_owner == Some(self.model.focus)
            && self.drawn.borrow().ime.accepts(&self.input)
    }
    pub(super) fn sync_password_state(&mut self) {
        if self.model.focus == Focus::Password {
            self.password_selection = self.input.ime_selection().range;
            self.password_reversed = self.input.reversed;
            self.password_marked = self.input.ime_marked();
        }
    }
    fn input_changed(&mut self, cx: &mut Context<Self>) {
        if self.model.focus == Focus::Password {
            self.sync_password_state();
            self.changed(cx);
        } else {
            self.edited(cx);
        }
    }
    fn ime_replace(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        marked: bool,
        cx: &mut Context<Self>,
    ) {
        let changed = if self.model.focus == Focus::Password {
            crate::g2::pl1::private_entry::replace(
                &mut self.input,
                &mut self.model.password,
                range,
                text,
                marked,
            )
        } else if marked {
            self.input.ime_mark(range, text, selected)
        } else {
            self.input.ime_replace(range, text)
        };
        if changed {
            self.input_changed(cx);
        }
    }
}
impl EntityInputHandler for NetworkView {
    fn accepts_text_input(&self, _: &mut Window, _: &mut Context<Self>) -> bool {
        self.input.composes()
    }
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        if !self.ime_active(window) {
            return None;
        }
        self.input.ime_text(&range, adjusted)
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        self.ime_active(window).then(|| self.input.ime_selection())
    }
    fn marked_text_range(
        &self,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.ime_active(window)
            .then(|| self.input.ime_marked())
            .flatten()
    }
    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.ime_active(window) {
            self.input.ime_unmark();
            self.input_changed(cx);
        }
    }
    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.ime_active(window) {
            self.ime_replace(range, text, None, false, cx);
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
        if self.ime_active(window) {
            self.ime_replace(range, text, selected, true, cx);
        }
    }
    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        if !self.ime_active(window) {
            return None;
        }
        let drawn = self.drawn.borrow();
        let text = drawn
            .text
            .iter()
            .find(|t| t.field == Some(self.model.focus))?;
        let bytes = if self.model.focus == Focus::Password {
            super::secret_edit::display_index(&self.model.password, range.start, self.model.reveal)
                ..super::secret_edit::display_index(
                    &self.model.password,
                    range.end,
                    self.model.reveal,
                )
        } else {
            self.input.ime_range(&range)
        };
        let (from, to) = drawn.ime.span(&bytes);
        let x = self.input_origin.get().x + gpui::px(drawn.origin.0 + text.rect.x);
        let y = self.input_origin.get().y + gpui::px(drawn.origin.1 + text.rect.y);
        Some(Bounds::from_corners(
            gpui::point(x + from, y),
            gpui::point(x + to, y + gpui::px(text.rect.h)),
        ))
    }
    fn character_index_for_point(
        &mut self,
        at: Point<Pixels>,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        self.ime_active(window).then(|| self.point_units(at))
    }
}
