//! The chooser's status bar: what is checked or said on the left, the keys on the right (PickerWindow.qml status).
use super::drawn::Slot;
use super::view::{GAP, SIDE, STATUS_HEIGHT, STATUS_TEXT};
use super::Picker;
use crate::list::Palette;
use gpui::{div, prelude::*, px, Context, Div, Hsla, SharedString, Window};

impl Picker {
    pub fn hints(&self) -> SharedString {
        if self.backend_unavailable() {
            self.labels.hints_unavailable.clone()
        } else if self.find.holds_query() {
            // While a query stands, esc clears it, so the hint says so (picker-filter board).
            self.labels.hints_filtered.clone()
        } else {
            self.labels.hints.clone()
        }
    }

    // PickerWindow.qml's statusHints: its own width until the message, held to at most half the bar, needs the room.
    fn hints_width(
        &self,
        message: Option<&super::picker::Message>,
        window: &Window,
        cx: &gpui::App,
    ) -> f32 {
        let font = crate::list::Typeface::for_window(window, cx).font;
        let said = match message {
            Some(message) => message.text.clone(),
            None if self.marks_allowed() => self.selection_status().into(),
            None => SharedString::default(),
        };
        let hints = super::answers::shaped_width(window, &font, self.hints(), STATUS_TEXT);
        let said = super::answers::shaped_width(window, &font, said, STATUS_TEXT);
        let bar = f32::from(window.viewport_size().width);
        hints.min((bar - 2. * SIDE - GAP - said.min(bar / 2.)).max(0.))
    }

    pub(super) fn footer(
        &self,
        palette: &Palette,
        theme: crate::palette::Theme,
        rule: Hsla,
        window: &Window,
        cx: &Context<Self>,
    ) -> Div {
        let message = self.message.clone();
        let hints_width = self.hints_width(message.as_ref(), window, cx);
        div()
            .debug_selector(|| "picker-status-band".into())
            .relative()
            .children(self.drawn.probe(Slot::Footer, [1., 0., 0., 0.]))
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .h(px(STATUS_HEIGHT))
            .px(px(SIDE))
            .text_size(px(STATUS_TEXT))
            .text_color(palette.foreground)
            .bg(theme.surface)
            .border_t_1()
            .border_color(rule)
            .child(
                div()
                    .debug_selector(|| "picker-status-message".into())
                    .flex_1()
                    .min_w_0()
                    .mr(px(GAP))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .when(
                        message.as_ref().is_some_and(|message| message.error),
                        |view| view.text_color(theme.error_on_surface),
                    )
                    .children(message.as_ref().map(|message| message.text.clone()))
                    // v0.3.12's footer says what is checked only where marks are allowed.
                    .when(message.is_none() && self.marks_allowed(), |view| {
                        view.child(
                            div()
                                .debug_selector(|| "picker-selection-status".into())
                                .child(self.selection_status()),
                        )
                    }),
            )
            .child(
                div()
                    .debug_selector(|| "picker-status-hints".into())
                    .flex_none()
                    .w(px(hints_width))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(self.hints()),
            )
    }
}
