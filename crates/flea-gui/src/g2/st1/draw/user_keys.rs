//! The built-in and user-bound key previews share one row recipe.
use super::*;
use gpui::TextRun;

pub(in crate::g2::st1) fn measure(
    line: &mut Line<'_>,
    typeface: &crate::list::Typeface,
    window: &gpui::Window,
) {
    if line.row.id == flea::ui_logic_st1::pages::USER_KEYS {
        line.keys_w = keys_width(line.row, typeface, window);
    }
}

/// The user chords' key column: the board's 48 unless a chord such as ctrl-shift-k shapes wider.
fn keys_width(row: &Row, typeface: &crate::list::Typeface, window: &gpui::Window) -> f32 {
    row.items
        .iter()
        .fold(Scale::at(typeface).of(super::KEYS_W), |widest, item| {
            let text = SharedString::from(item.keys.clone());
            let run = TextRun {
                len: text.len(),
                font: typeface.font.clone(),
                color: Default::default(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let shaped = window
                .text_system()
                .shape_line(text, typeface.caption, &[run], None)
                .width;
            widest.max(f32::from(shaped).ceil())
        })
}

impl Rows<'_> {
    pub(super) fn key_preview(
        &self,
        base: gpui::Stateful<Div>,
        row: &Row,
        keys_w: f32,
    ) -> gpui::Stateful<Div> {
        let ink = self.ink;
        let s = self.s();
        let keys_w = keys_w.max(s.of(super::KEYS_W));
        let cell = (self.view.panel.pane.0 - 2. * s.of(PAD_X) - s.of(GAP)) / 2.;
        base.flex()
            .flex_wrap()
            .gap(self.px(GAP))
            .py(self.px(7.))
            .px(self.px(PAD_X))
            .text_size(self.view.typeface.caption)
            .children(row.items.iter().map(|item| {
                div()
                    .w(px(cell))
                    .h(self.px(CHIP_H))
                    .flex()
                    .items_center()
                    .gap(self.px(GAP))
                    .child(
                        div()
                            .w(px(keys_w))
                            .mr(self.px(5.))
                            .flex()
                            .justify_end()
                            .text_color(ink.foreground)
                            .child(SharedString::from(item.keys.clone())),
                    )
                    .when(!item.glyph.is_empty(), |cell| {
                        cell.child(mark(item.glyph, s.of(16.), ink.muted_frame))
                    })
                    .child(div().whitespace_nowrap().text_color(ink.foreground).child(
                        SharedString::from(item.label.resolve(&self.view.panel.catalog)),
                    ))
            }))
    }
}
