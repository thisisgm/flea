//! The built-in and user-bound key previews share one row recipe.
use super::*;
use flea::i18n::I18n;
use flea::ui_logic_settings_pages::rows::KeyItem;
use gpui::TextRun;

pub(in crate::g2::settings_pages) fn measure(
    line: &mut Line<'_>,
    typeface: &crate::list::Typeface,
    catalog: &I18n,
    window: &gpui::Window,
) {
    // The built-in examples keep the board's column for their chords; only a translated word may widen it.
    let user = line.row.id == flea::ui_logic_settings_pages::pages::USER_KEYS;
    if user || line.row.kind == Kind::KeyPreview {
        line.keys_w = keys_width(line.row, user, typeface, catalog, window);
    }
}

/// What an example's key cell draws: its keys, or the catalog's word for keys that are prose.
fn keys_text(item: &KeyItem, catalog: &I18n) -> String {
    if item.word.is_empty() {
        item.keys.clone()
    } else {
        item.word.resolve(catalog)
    }
}

/// The key column: the board's 48 unless a chord such as ctrl-shift-k or a translated word shapes wider.
fn keys_width(
    row: &Row,
    user: bool,
    typeface: &crate::list::Typeface,
    catalog: &I18n,
    window: &gpui::Window,
) -> f32 {
    row.items
        .iter()
        .filter(|item| user || !item.word.is_empty())
        .fold(Scale::at(typeface).of(super::KEYS_W), |widest, item| {
            let text = SharedString::from(keys_text(item, catalog));
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
            .flex_col()
            .gap(self.px(GAP))
            .py(self.px(7.))
            .px(self.px(PAD_X))
            .text_size(self.view.typeface.caption)
            .children(
                row.items
                    .chunks(KEY_COLUMNS)
                    .enumerate()
                    .map(|(band, items)| {
                        div().flex_1().min_h_0().flex().gap(self.px(GAP)).children(
                            items.iter().enumerate().map(|(at, item)| {
                                let (id, label_id) = (row.id.clone(), row.id.clone());
                                let at = band * KEY_COLUMNS + at;
                                div()
                                    .debug_selector(move || {
                                        format!("settings-key-preview-{id}-{at}")
                                    })
                                    .w(px(cell))
                                    .h_full()
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
                                            .child(SharedString::from(keys_text(
                                                item,
                                                &self.view.panel.catalog,
                                            ))),
                                    )
                                    .when(!item.glyph.is_empty(), |cell| {
                                        cell.child(mark(item.glyph, s.of(16.), ink.muted_frame))
                                    })
                                    .child(
                                        div()
                                            .debug_selector(move || {
                                                format!("settings-key-label-{label_id}-{at}")
                                            })
                                            .whitespace_nowrap()
                                            .text_color(ink.foreground)
                                            .child(SharedString::from(
                                                item.label.resolve(&self.view.panel.catalog),
                                            )),
                                    )
                            }),
                        )
                    }),
            )
    }
}
