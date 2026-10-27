use super::{
    geometry::{self as g, HEAD_H, PAD_X},
    marks::mark,
    seams::{Ink, Request},
    view::SettingsView,
};
use gpui::{div, prelude::*, px, Context, Div, FontWeight, SharedString};

impl SettingsView {
    /// The head: the title's sliders mark in its own ink, the pane's name, esc and the close mark.
    pub(super) fn head(&self, ink: &Ink, cx: &mut Context<Self>) -> Div {
        div()
            .h(px(HEAD_H))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(g::GAP))
            .px(px(PAD_X))
            .border_b_1()
            .border_color(ink.rule)
            .text_size(self.typeface.caption)
            .text_color(ink.foreground)
            .child(mark("sliders", 16., ink.foreground))
            .child(
                // The probe boxes the title text itself, so its centre is the drawn title's, not the head row's.
                div().flex_1().flex().child(
                    self.probe.attach(
                        div()
                            .flex_none()
                            .font_weight(FontWeight::BOLD)
                            .whitespace_nowrap()
                            .child(SharedString::from(self.panel.title())),
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
                    .child(mark("x", 16., ink.foreground))
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.panel.requests.push(Request::Close);
                        view.flush(cx);
                    })),
            )
    }
}
