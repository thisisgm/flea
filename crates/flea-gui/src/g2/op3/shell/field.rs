//! The shared search and entry drawing, including mounted IME input and the clear control.
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
        let slot = || crate::g2::op1::field::slot(&field.value, &field.placeholder, t.muted);
        // A Shell card names its input; a live card's own entity mounts its focused field.
        let mounted = match (&field.input, self.mount) {
            (Some(input), _) => Some(
                crate::g2::op1::field::mount(input.clone(), slot(), self.typeface)
                    .into_any_element(),
            ),
            (None, Some(mount)) if field.focused => Some(mount(slot())),
            _ => None,
        };
        let mut line = if let Some(mounted) = mounted {
            line.child(mounted)
        } else {
            let mut text = div()
                .flex()
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
            } else {
                text = text.child(field.value.clone());
                if field.focused {
                    text = text.child(caret());
                }
            }
            line.child(crate::locale_geometry::unmirrored(text))
        };
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
