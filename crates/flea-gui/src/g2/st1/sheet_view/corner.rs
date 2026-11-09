//! The title's corner: what the keyboard shows held or looked up, the key that opens its search, Escape and the close
//! mark; and the card's hint when the window is too small for the keyboard.
use super::super::scroll::Measured;
use super::super::{SheetView, PAD, TITLE_H};
use super::held;
use crate::g2::st1::{
    geometry::GAP,
    marks::mark,
    seams::{Ink, Request},
};
use flea::ui_logic_st1::sheet_keyboard::held_words;
use gpui::{div, prelude::*, px, Context, Div, SharedString, Window};

impl SheetView {
    /// A key let go in another window sends no key-up here, so losing the window forgets what was held.
    pub(crate) fn forget_on_blur(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.blur = Some(cx.observe_window_activation(window, |view, window, cx| {
            if !window.is_window_active() {
                view.sheet.forget();
                cx.notify();
            }
        }));
    }

    /// The title's corner: its hint and the close mark Settings has, which closes the sheet as Escape does.
    pub(in crate::g2::st1::sheet_view) fn corner(
        &self,
        window: &Window,
        ink: &Ink,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .flex()
            .items_center()
            .gap(px(GAP))
            .child(self.corner_hint(window, ink))
            .child(
                div()
                    .id("sheet-close")
                    .debug_selector(|| "sheet-close".into())
                    .size(px(TITLE_H))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .child(mark("x", 16., ink.foreground))
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.sheet.requests.push(Request::Close);
                        view.flush(cx);
                    })),
            )
    }

    /// In a window too small for the keyboard the card holds only the hint to make it larger; otherwise the rows the
    /// held modifiers or a looked-up chord list, if any.
    pub(in crate::g2::st1::sheet_view) fn keyboard_body(
        &self,
        window: &Window,
        measured: &Measured,
        ink: &Ink,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        if !self.too_small(window) {
            return self.held_rows(window, measured, ink, cx);
        }
        let larger = self.sheet.catalog.tr("keys.hint.keyboard");
        Some(
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .px(px(PAD))
                .text_color(ink.muted)
                // A long language's hint wraps within the card rather than running past it.
                .child(
                    div()
                        .w_full()
                        .min_w_0()
                        .text_center()
                        .child(SharedString::from(larger)),
                ),
        )
    }

    /// What the corner reads: the held modifiers or looked-up chord; at rest the key that opens the search; while
    /// searching, in a window too small for the keyboard, and without one, the sheet's own Escape hint.
    fn corner_hint(&self, window: &Window, ink: &Ink) -> Div {
        let sheet = &self.sheet;
        // Without a drawn keyboard the corner keeps the sheet's own Escape hint.
        let (Some(keyboard), Some(_)) = (sheet.keyboard.as_ref(), self.unit(window)) else {
            return div().child("esc");
        };
        // A caption's baseline sits 2px above the bold title's in the same row.
        let hint = div().relative().top(px(2.)).flex().gap(px(16.));
        match sheet.chord {
            Some(cap) => hint.child(held::chord_line(&keyboard.chord(sheet.held, cap), ink)),
            None if sheet.held != 0 => {
                hint.child(held::chord_line(&held_words(sheet.held).join(" + "), ink))
            }
            None if !self.searching() => hint
                .child(
                    div()
                        .flex()
                        .gap(px(GAP))
                        .child(held::chord_line("CTRL + F", ink))
                        .child(SharedString::from(sheet.catalog.tr("keys.action.search"))),
                )
                .child("esc"),
            None => hint.child("esc"),
        }
    }
}
