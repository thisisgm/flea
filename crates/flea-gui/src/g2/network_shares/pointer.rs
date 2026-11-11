//! Pointer caret placement and selection share the keyboard editor's UTF-16 positions.
use super::{dialog::Focus, view::NetworkView};
use gpui::{Context, Pixels, Point};

impl NetworkView {
    pub(super) fn point_units(&self, at: Point<Pixels>) -> usize {
        let drawn = self.drawn.borrow();
        let Some(text) = drawn
            .text
            .iter()
            .find(|text| text.field == Some(self.model.focus))
        else {
            return 0;
        };
        let byte = text.run.line().closest_index_for_x(self.run_x(text, at));
        if self.model.focus == Focus::Password {
            super::secret_edit::units_at_display_index(
                &self.model.password,
                byte,
                self.model.reveal,
            )
        } else {
            self.input.offset_to_utf16(byte)
        }
    }
    fn run_x(&self, text: &super::drawing::Text, at: Point<Pixels>) -> Pixels {
        let origin = self.drawn.borrow().origin.0;
        at.x - self.input_origin.get().x - gpui::px(origin + text.rect.x)
    }
    // A double-click selects the word of the glyph under the pointer, a third click or any click in the password the whole line.
    fn select_word(&mut self, at: Point<Pixels>, clicks: usize, cx: &mut Context<Self>) {
        let drawn = self.drawn.borrow();
        let text = drawn
            .text
            .iter()
            .find(|text| text.field == Some(self.model.focus));
        let under = text.and_then(|text| text.run.line().index_for_x(self.run_x(text, at)));
        drop(drawn);
        if clicks == 2 && self.model.focus != Focus::Password {
            self.input
                .select_word(under.map_or(self.input.text.len(), |at| self.input.from_display(at)));
        } else {
            self.input.select_all();
        }
        self.sync_password_state();
        self.paint_changed(cx);
    }
    pub(super) fn text_down(
        &mut self,
        focus: Focus,
        at: Point<Pixels>,
        (shift, clicks): (bool, usize),
        cx: &mut Context<Self>,
    ) -> bool {
        if super::drawing::field(focus).is_none()
            || !self.model.ring().contains(&focus)
            || !self.model.opened
            || self.model.busy()
            || self.model.committed
        {
            return false;
        }
        if self.model.focus != focus {
            self.model.focus = focus;
            self.load_input();
            self.changed(cx);
        }
        if clicks > 1 {
            self.selecting = None;
            self.select_word(at, clicks, cx);
            return true;
        }
        let at = self.point_units(at);
        let anchor = if shift {
            if focus == Focus::Password {
                if self.password_reversed {
                    self.password_selection.end
                } else {
                    self.password_selection.start
                }
            } else {
                self.input.offset_to_utf16(if self.input.reversed {
                    self.input.selected.end
                } else {
                    self.input.selected.start
                })
            }
        } else {
            at
        };
        self.selecting = Some(anchor);
        self.select_text(anchor, at, cx);
        true
    }
    pub(super) fn text_drag(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) -> bool {
        if !self.model.opened || self.model.busy() || self.model.committed {
            self.selecting = None;
            return false;
        }
        let Some(anchor) = self.selecting else {
            return false;
        };
        self.select_text(anchor, self.point_units(at), cx);
        true
    }
    fn select_text(&mut self, anchor: usize, at: usize, cx: &mut Context<Self>) {
        self.input.move_to(self.input.offset_from_utf16(anchor));
        self.input.select_to(self.input.offset_from_utf16(at));
        self.sync_password_state();
        self.paint_changed(cx);
    }
}
