//! Hint rows preserve the producer's wrapping or one-line elision policy.
use super::{g, Line, Rows, PAD_X};
use gpui::{div, prelude::*, px, Div, SharedString};

impl Rows<'_> {
    pub(super) fn hint(&self, base: gpui::Stateful<Div>, line: &Line<'_>) -> gpui::Stateful<Div> {
        let row = line.row;
        let ink = self.role_ink(row.role);
        // A wrapping hint is placed unit by unit so a key chord never breaks; elided hints and typed runs keep the label.
        let text = if row.elide_right || line.run.is_some() {
            self.label(line, ink, false).map(|text| {
                if row.elide_right {
                    text.w_full().whitespace_nowrap().text_ellipsis()
                } else {
                    text.flex_wrap().whitespace_normal()
                }
            })
        } else {
            div()
                .flex()
                .flex_wrap()
                .children(g::hint_units(&self.text(row)).into_iter().map(|unit| {
                    let name = format!("st1-hint-unit-{}", unit.trim_end());
                    div()
                        .debug_selector(move || name)
                        .flex_none()
                        .whitespace_nowrap()
                        .child(SharedString::from(unit))
                }))
        };
        let base = base.text_size(self.view.typeface.caption).text_color(ink);
        if row.footer {
            return base.child(
                div()
                    .mt(self.px(10.))
                    .border_t_1()
                    .border_color(self.ink.hair)
                    .pt(self.px(10.))
                    .px(self.px(PAD_X))
                    .pb(self.px(12.))
                    .line_height(self.px(g::FOOTER_LINE))
                    .child(text),
            );
        }
        base.pt(self.px(4.))
            .pl(px(self.s().label_x()))
            .pr(self.px(PAD_X))
            .pb(self.px(2.))
            .line_height(self.px(g::HINT_LINE))
            .child(text)
    }
}
