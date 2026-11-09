//! Selection and preedit geometry reuse the shared shaped-line mapping.
use super::*;
impl Drawn {
    pub(in crate::g2::network_shares) fn caret(
        &mut self,
        focus: Focus,
        rect: Rect,
        view: &NetworkView,
    ) {
        if let Some(text) = self.text.last_mut() {
            text.field = Some(focus);
        }
        if view.model.focus != focus || view.model.busy() || view.model.committed {
            return;
        }
        let Some(text) = self.text.last_mut() else {
            return;
        };
        let selected = if focus == Focus::Password {
            crate::g2::network_shares::secret_edit::display_index(
                &view.model.password,
                view.password_selection.start,
                view.model.reveal,
            )
                ..crate::g2::network_shares::secret_edit::display_index(
                    &view.model.password,
                    view.password_selection.end,
                    view.model.reveal,
                )
        } else {
            view.input.selected.clone()
        };
        let cursor_at = if focus == Focus::Password {
            if view.password_reversed {
                selected.start
            } else {
                selected.end
            }
        } else {
            view.input.cursor()
        };
        self.ime.run = Some(std::rc::Rc::new(GlyphRun::new(text.run.line().clone())));
        if focus == Focus::Password && view.model.reveal {
            self.ime.set_text_direction(
                std::str::from_utf8(view.model.password.expose()).unwrap_or(""),
            );
        } else {
            self.ime.set_text_direction(&view.input.text);
        }
        let cursor = f32::from(self.ime.caret_x(cursor_at));
        let missing = (WIDTH - self.card.map_or(WIDTH, |card| card.w)).max(0.);
        let adjustment = if rect.w == 226.5 {
            missing / 2.
        } else {
            missing
        };
        let room = (rect.w - if focus == Focus::Password { 42. } else { 20. } - adjustment).max(0.);
        let scroll = (cursor - room).max(0.);
        text.clip = Some(Rect {
            x: rect.x + 1.,
            w: rect.w - 2.,
            ..rect
        });
        text.rect.x -= scroll;
        let mut add = |x: f32, w: f32, h: f32, y: f32, ink| {
            self.boxes.push(BoxPaint {
                field: Some(focus),
                rect: Rect { x, y, w, h },
                fill: ink,
                border: ink,
                radius: 0.,
                border_width: 1.,
                clip: None,
            });
        };
        let marked = if focus == Focus::Password {
            view.password_marked.as_ref().map(|range| {
                crate::g2::network_shares::secret_edit::display_index(
                    &view.model.password,
                    range.start,
                    view.model.reveal,
                )
                    ..crate::g2::network_shares::secret_edit::display_index(
                        &view.model.password,
                        range.end,
                        view.model.reveal,
                    )
            })
        } else {
            view.input.marked.clone()
        };
        if let Some(marked) = marked {
            let (from, to) = self.ime.span(&marked);
            let (from, to) = (
                (f32::from(from) - scroll).max(0.),
                (f32::from(to) - scroll).min(room),
            );
            if to > from {
                add(
                    rect.x + 10. + from,
                    to - from,
                    1.,
                    rect.y + 23.,
                    view.palette.foreground,
                );
            }
        }
        if !selected.is_empty() {
            let (from, to) = self.ime.span(&selected);
            let (from, to) = (f32::from(from) - scroll, f32::from(to) - scroll);
            let from = from.max(0.);
            let to = to.min(room);
            if to > from {
                add(
                    rect.x + 10. + from,
                    to - from,
                    17.,
                    rect.y + 6.,
                    view.palette.accent.opacity(0.24),
                );
            }
        } else {
            add(
                rect.x + 10. + cursor - scroll,
                1.,
                17.,
                rect.y + 6.,
                view.palette.foreground,
            );
        }
    }
}
