//! Hint rows preserve the producer's wrapping or one-line elision policy.
use super::{g, Line, Rows, LABEL_X, PAD_X};
use gpui::{div, prelude::*, px, Div};

impl Rows<'_> {
    pub(super) fn hint(&self, base: gpui::Stateful<Div>, line: &Line<'_>) -> gpui::Stateful<Div> {
        let row = line.row;
        let ink = self.role_ink(row.role);
        let text = self.label(line, ink).map(|text| {
            if row.elide_right {
                text.w_full().whitespace_nowrap().text_ellipsis()
            } else {
                text.flex_wrap().whitespace_normal()
            }
        });
        let base = base.text_size(self.view.typeface.caption).text_color(ink);
        if row.footer {
            return base.child(
                div()
                    .mt(px(10.))
                    .border_t_1()
                    .border_color(self.ink.hair)
                    .pt(px(10.))
                    .px(px(PAD_X))
                    .pb(px(12.))
                    .line_height(px(g::FOOTER_LINE))
                    .child(text),
            );
        }
        base.pt(px(4.))
            .pl(px(LABEL_X))
            .pr(px(PAD_X))
            .pb(px(2.))
            .line_height(px(g::HINT_LINE))
            .child(text)
    }
}
