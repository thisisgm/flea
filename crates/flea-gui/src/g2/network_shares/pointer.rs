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
        let byte = text.run.line().closest_index_for_x(
            at.x - self.input_origin.get().x - gpui::px(drawn.origin.0 + text.rect.x),
        );
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
    pub(super) fn text_down(
        &mut self,
        focus: Focus,
        at: Point<Pixels>,
        shift: bool,
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
