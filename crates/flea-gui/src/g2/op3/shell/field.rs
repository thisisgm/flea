//! The shared search and entry drawing, with the shipped Open With search clear control.
use super::*;
use gpui::{div, prelude::*, px, Div};

impl Draw<'_> {
    pub(super) fn field(&self, field: &Field) -> Div {
        let t = self.t;
        let caret = || {
            div()
                .relative()
                .w(px(0.))
                .h(self.px(16.))
                .flex_none()
                .child(
                    div()
                        .debug_selector(|| "op3-caret".into())
                        .absolute()
                        .left_0()
                        .top_0()
                        .w(px(1.))
                        .h_full()
                        .bg(t.fg),
                )
        };
        let frame = if field.focused {
            t.accent_frame
        } else {
            t.muted_frame
        };
        if let Some(probe) = self.probe {
            let role = if frame == t.error {
                "error"
            } else if frame == t.accent {
                "accent"
            } else if frame == t.muted_frame {
                "mutedFrame"
            } else if frame == t.muted {
                "muted"
            } else {
                "other"
            };
            probe.set_role("field", role);
        }
        let mut line = div()
            .h(self.px(BUTTON_HEIGHT))
            .flex()
            .items_center()
            .gap(self.px(8.))
            .px(self.px(BUTTON_PAD))
            .bg(t.ground)
            .border_1()
            .border_color(frame)
            .rounded(px(self.corner))
            .text_color(t.fg)
            .whitespace_nowrap();
        if let Some(mark) = field.glyph {
            line = line.child(glyph(mark, 14. * self.s, t.muted));
        }
        if let Some(input) = &field.input {
            line = line.child(crate::g2::op1::field::mount(
                input.clone(),
                div().relative().flex_1().min_w_0().h_full().when(
                    field.value.is_empty() && !field.placeholder.is_empty(),
                    |slot| {
                        slot.child(
                            div()
                                .absolute()
                                .left(px(1.))
                                .top_0()
                                .h_full()
                                .flex()
                                .items_center()
                                .text_color(t.muted)
                                .child(field.placeholder.clone()),
                        )
                    },
                ),
                self.typeface,
            ));
            return match &field.label {
                Some(name) => div().flex().flex_col().child(self.label(name)).child(line),
                None => line,
            };
        }
        let mut text = div()
            .flex()
            .flex_1()
            .items_center()
            .min_w_0()
            .pr(px(1.))
            .overflow_hidden();
        if field.value.is_empty() {
            if field.focused {
                text = text.child(caret());
            }
            if !field.placeholder.is_empty() {
                text = text.child(
                    div()
                        .ml(px(1.))
                        .text_color(t.muted)
                        .child(field.placeholder.clone()),
                );
            }
        } else if let Some(entry) = self
            .entry
            .filter(|entry| field.focused && entry.text == field.value)
        {
            let selected = entry.selected.clone();
            text = text.child(field.value[..selected.start].to_owned());
            if selected.is_empty() {
                text = text.child(caret());
            } else {
                text = text.child(
                    div()
                        .debug_selector(|| "op3-selection".into())
                        .bg(t.accent.opacity(flea::ui_logic_op1::card::FIELD_WASH))
                        .child(field.value[selected.clone()].to_owned()),
                );
            }
            text = text.child(field.value[selected.end..].to_owned());
        } else {
            text = text.child(field.value.clone());
            if field.focused {
                text = text.child(caret());
            }
        }
        let mut line = line.child(text);
        if field.glyph == Some(Glyph::Search) && !field.value.is_empty() {
            line = line.child(
                self.wired(
                    Control::Clear,
                    self.track(
                        div()
                            .size(self.px(CAPTION))
                            .flex_none()
                            .mr(self.px(GAP - BUTTON_PAD))
                            .child(glyph(Glyph::Cross, CAPTION * self.s, t.muted_frame)),
                        "clear",
                    )
                    .id("op3-clear")
                    .debug_selector(|| "op3-clear".into()),
                ),
            );
        }
        match &field.label {
            Some(name) => div().flex().flex_col().child(self.label(name)).child(line),
            None => line,
        }
    }
}
