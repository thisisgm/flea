use super::{
    geometry::{self as g, Scale, PAD_X},
    marks::{mark, washed},
    seams::{Ink, Request},
    view::SettingsView,
};
use crate::list::Typeface;
use flea::ui_logic_st1::caps;
use gpui::{div, prelude::*, px, Context, Div, FontWeight, MouseButton, SharedString};

impl SettingsView {
    /// The search field the sheet uses: magnifier, the typed text and caret, and the clear mark at its end.
    pub(super) fn field(text: &str, ink: &Ink, corner: f32, typeface: &Typeface, s: Scale) -> Div {
        div()
            .h(px(s.of(g::FIELD_H)))
            .flex()
            .items_center()
            .gap(px(s.of(8.)))
            .px(px(s.of(g::GAP)))
            .border_1()
            .border_color(ink.accent_frame)
            .rounded(px(corner))
            .bg(ink.background)
            .text_size(typeface.body)
            .text_color(ink.foreground)
            .child(mark("search", s.of(14.), ink.muted))
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(SharedString::from(text.to_owned()))
                    .child(
                        div()
                            .ml(px(1.))
                            .w(px(1.))
                            .h(px(s.of(16.)))
                            .bg(ink.foreground),
                    ),
            )
            .child(div().flex_1())
            .child(mark("x", s.of(12.), ink.muted))
    }

    /// The head: the title's sliders mark in its own ink, the pane's name, esc and the close mark.
    pub(super) fn head(&self, ink: &Ink, cx: &mut Context<Self>) -> Div {
        let s = self.panel.scale;
        div()
            .h(px(s.head))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(s.of(g::GAP)))
            .px(px(s.of(PAD_X)))
            .border_b_1()
            .border_color(ink.rule)
            .text_size(self.typeface.caption)
            .text_color(ink.foreground)
            .child(mark("sliders", s.of(16.), ink.foreground))
            .child(
                // The probe boxes the title text itself, so its centre is the drawn title's, not the head row's.
                div().flex_1().flex().child(
                    self.probe.attach(
                        div()
                            .flex_none()
                            .font_weight(FontWeight::BOLD)
                            .whitespace_nowrap()
                            .child(SharedString::from(
                                self.plugins
                                    .as_ref()
                                    .filter(|_| {
                                        self.panel.section == "plugins" && !self.panel.searching()
                                    })
                                    .map_or_else(
                                        || self.panel.title(),
                                        |plugin| plugin.read(cx).page().title(),
                                    ),
                            )),
                        "title",
                    ),
                ),
            )
            .child(div().text_color(ink.muted).child("esc"))
            .child(
                div()
                    .id("settings-close")
                    .size(px(24.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .child(mark("x", s.of(16.), ink.foreground))
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.panel.requests.push(Request::Close);
                        view.flush(cx);
                    })),
            )
    }

    /// A heading in the capitals its language writes, spaced as the boards space it; a typed run keeps its wash.
    pub(super) fn heading(
        &self,
        label: &str,
        run: Option<(usize, usize)>,
        lifted: bool,
        ink: &Ink,
    ) -> Div {
        let language = self.panel.catalog.language();
        let upper = |text: &str| caps::heading(text, language);
        let spacing = f32::from(self.typeface.caption) * 0.14;
        let Some((start, length)) = run else {
            return washed(&upper(label), None, lifted, ink.wash14, spacing);
        };
        // Capitals can change a run's length, as ß does, so each part is set in capitals on its own.
        let at = |chars: usize| {
            label
                .char_indices()
                .nth(chars)
                .map_or(label.len(), |(byte, _)| byte)
        };
        let (from, to) = (at(start), at(start + length));
        let (head, piece, tail) = (
            upper(&label[..from]),
            upper(&label[from..to]),
            upper(&label[to..]),
        );
        let run = (head.chars().count(), piece.chars().count());
        washed(
            &format!("{head}{piece}{tail}"),
            Some(run),
            lifted,
            ink.wash14,
            spacing,
        )
    }
}

impl SettingsView {
    /// Plugin dialogs center over the Settings window and retain the plugin keyboard owner.
    pub(super) fn plugin_card(&self, cx: &gpui::App) -> Option<Div> {
        self.plugins
            .as_ref()
            .filter(|_| self.panel.section == "plugins" && !self.panel.searching())
            .and_then(|plugin| plugin.read(cx).card())
            .map(|card| {
                div()
                    .absolute()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                    .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                    .child(card)
            })
    }
}
