//! The Add and Remove cards' bodies, drawn inside op1's dialog shell once it joins.
use super::mark::{metrics as m, tracked};
use super::page::{Card, Key};
use super::view::PluginsPane;
use crate::g2::settings_pages::input::Field;
use gpui::{div, prelude::*, px, Div, Entity, FontWeight, SharedString, Stateful};

impl PluginsPane {
    /// A click presses the button as its key would: Cancel as Escape, the card's action as Enter.
    fn button(
        &self,
        label: &str,
        (primary, available, destructive): (bool, bool, bool),
        (pane, key): (Entity<Self>, Key),
    ) -> Stateful<Div> {
        let t = self.tokens;
        let lit = primary && available;
        let id = if key == Key::Escape {
            "plugins-card-cancel"
        } else {
            "plugins-card-action"
        };
        div()
            .id(id)
            .debug_selector(move || id.into())
            .h(px(m::FIELD))
            .px(px(m::FIELD_PAD))
            .flex()
            .items_center()
            .text_size(px(m::BODY))
            .text_color(if destructive { t.error } else { t.fg })
            .border_1()
            .rounded(px(self.radius))
            .border_color(if lit { t.accent_frame } else { t.frame() })
            .when(lit, |b| b.bg(t.wash14()))
            .when(!available, |b| b.opacity(m::DISABLED))
            .child(SharedString::from(label.to_string()))
            .on_click(move |_, _, cx| pane.update(cx, |pane, cx| pane.key(key.clone(), cx)))
    }

    // The card body inside op1's dialog shell: title row, hairline, its parts, then the buttons.
    pub(crate) fn card(&self, pane: gpui::Entity<Self>) -> Option<Div> {
        let t = self.tokens;
        let i18n = self.page.catalog();
        let (title, body, buttons): (String, Vec<Div>, Div) = match self.page.card() {
            Card::None => return None,
            Card::Add(card) => {
                let field = div()
                    .h(px(m::FIELD))
                    .px(px(m::FIELD_PAD))
                    .flex()
                    .items_center()
                    .text_size(px(m::BODY))
                    .text_color(t.fg)
                    .bg(t.bg)
                    .border_1()
                    .border_color(t.accent_frame)
                    .rounded(px(self.radius))
                    .child(Field::mount(pane.clone(), &self.input_typeface, true));
                let label = tracked(
                    &i18n.tr("plugins.add.field").to_uppercase(),
                    m::FIELD_TRACKING,
                )
                .text_size(px(m::CAPTION))
                .line_height(px(14.))
                .text_color(t.muted)
                .mb(px(m::FIELD_LABEL_GAP));
                let mut parts = vec![div().child(label).child(field)];
                parts.push(
                    div()
                        .text_size(px(m::CAPTION))
                        .line_height(gpui::relative(1.5))
                        .text_color(t.muted)
                        .child(SharedString::from(i18n.tr("plugins.add.warning"))),
                );
                if let Some(error) = &card.error {
                    parts.push(
                        div()
                            .text_size(px(m::CAPTION))
                            .line_height(gpui::relative(1.5))
                            .text_color(t.error)
                            .child(SharedString::from(error.clone())),
                    );
                }
                let buttons = div()
                    .flex()
                    .justify_end()
                    .items_center()
                    .gap(px(m::GAP))
                    .mt(px(m::BUTTONS_ABOVE))
                    .child(self.button(
                        &i18n.tr("plugins.add.cancel"),
                        (false, true, false),
                        (pane.clone(), Key::Escape),
                    ))
                    .child(self.button(
                        &i18n.tr("plugins.add.confirm"),
                        (true, card.can_add(), false),
                        (pane.clone(), Key::Enter),
                    ));
                (i18n.tr("plugins.add.title"), parts, buttons)
            }
            Card::Remove {
                name, busy, error, ..
            } => {
                let mut parts = vec![div()
                    .text_size(px(m::CAPTION))
                    .line_height(gpui::relative(1.5))
                    .text_color(t.muted)
                    .child(SharedString::from(i18n.tr("plugins.remove.body")))];
                if let Some(error) = error {
                    parts.push(
                        div()
                            .text_size(px(m::CAPTION))
                            .line_height(gpui::relative(1.5))
                            .text_color(t.error)
                            .child(SharedString::from(error.clone())),
                    );
                }
                let buttons = div()
                    .flex()
                    .justify_end()
                    .items_center()
                    .gap(px(m::GAP))
                    .mt(px(m::BUTTONS_ABOVE))
                    .child(self.button(
                        &i18n.tr("plugins.add.cancel"),
                        (false, true, false),
                        (pane.clone(), Key::Escape),
                    ))
                    .child(self.button(
                        &i18n.tr("plugins.remove.confirm"),
                        (true, !busy, true),
                        (pane.clone(), Key::Enter),
                    ));
                (
                    i18n.tr_args("plugins.remove.title", &[name]),
                    parts,
                    buttons,
                )
            }
        };
        let header = div()
            .flex()
            .items_center()
            .gap(px(m::GAP))
            .text_size(px(m::CARD_TITLE))
            .font_weight(FontWeight::BOLD)
            .text_color(t.fg)
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(SharedString::from(title)),
            )
            .child(
                div()
                    .text_size(px(m::CAPTION))
                    .font_weight(FontWeight::NORMAL)
                    .text_color(t.muted)
                    .child(SharedString::from(i18n.tr("plugins.card.esc"))),
            );
        Some(
            div()
                .flex_none()
                .w(px(m::CARD_WIDTH))
                .p(px(m::CARD_PAD))
                .flex()
                .flex_col()
                .gap(px(m::CARD_GAP))
                .bg(t.surface)
                .border_1()
                .border_color(t.frame())
                .rounded(px(self.radius))
                .child(header)
                .child(div().h(px(1.)).bg(t.rule()))
                .children(body)
                .child(buttons),
        )
    }
}
