//! The existing answer row gains its leading actions without changing the IME-backed Filename.
use super::filename::{LABEL, WIDTH as FIELD};
use super::view::{BUTTON_HEIGHT, BUTTON_SIDE, GAP, SIDE, TEXT};
use super::Picker;
use crate::g2::op1::glyph::{Glyph, Paint};
use crate::list::Palette;
use gpui::{div, prelude::*, px, Context, Div, MouseButton, SharedString, Window};

// picker-filter: the trigger is as wide as the menu card it opens, with a 16 px chevron in 80% ink.
const FILTER_WIDTH: f32 = flea::ui_logic_mn1::fit::MENU_W;
const CHEVRON: f32 = 16.;
const CHEVRON_INK: f32 = 0.8;
const FILTER_INSET: f32 = 8.;
// A refused trigger dims as the path controls do, and hover lays the 8% foreground wash.
const REFUSED: f32 = 0.55;
const HOVER: f32 = 0.08;
// A DialogButton's 1 px frame, outside its text padding.
const FRAME: f32 = 1.;
// picker-save's spacer after New Folder…, so 24 px stand between it and the label.
const SPACER: f32 = 6.;

impl Picker {
    pub(super) fn answer_leading(
        &mut self,
        palette: &Palette,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut row = div()
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(9.))
            .text_size(px(14.));
        if flea::ui_logic_pk1::saving(&self.req) {
            let focused = self.chrome.new_folder.is_focused(window);
            row = row.child(
                self.chrome
                    .probe
                    .attach(div().relative(), "picker-new-folder")
                    .id("picker-new-folder")
                    .debug_selector(|| "picker-new-folder".into())
                    .track_focus(&self.chrome.new_folder)
                    .flex_none()
                    .h(px(30.))
                    .px(px(9.))
                    .flex()
                    .items_center()
                    .rounded(px(crate::compositor::corner(cx)))
                    .border_1()
                    .border_color(if focused {
                        palette.foreground
                    } else {
                        crate::presentation::frames::Frames::from_palette(palette, 0.).ink
                    })
                    .child(self.locale.catalog.tr("picker.browser.new_folder"))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(
                        cx.listener(|picker, _, window, cx| picker.open_new_folder(window, cx)),
                    ),
            );
            row = row.child(div().flex_none().w(px(SPACER)));
        }
        row = row.children(self.filter.clone());
        if !self.filter_stacks(window, cx) {
            row = row.children(self.filter_trigger(palette, window, cx));
        }
        row
    }

    // The caller's filter trigger, absent when the request names no filters.
    pub(super) fn filter_trigger(
        &mut self,
        palette: &Palette,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Stateful<Div>> {
        if self.req.filters.is_empty() {
            return None;
        }
        let focused = self.chrome.filter.is_focused(window);
        let enabled = !self.backend_unavailable() && !self.accept_state().submitting();
        Some(
            self.chrome
                .probe
                .attach(div().relative(), "picker-filter")
                .id("picker-filter")
                .debug_selector(|| "picker-filter".into())
                .track_focus(&self.chrome.filter)
                .w(px(FILTER_WIDTH))
                .flex_none()
                .min_w_0()
                .h(px(BUTTON_HEIGHT))
                .px(px(FILTER_INSET))
                .flex()
                .items_center()
                .gap(px(GAP))
                .rounded(px(crate::compositor::corner(cx)))
                .when(!enabled, |view| view.opacity(REFUSED))
                .hover(|view| view.bg(palette.foreground.opacity(HOVER)))
                .border_1()
                .border_color(if focused {
                    palette.accent
                } else {
                    crate::presentation::frames::Frames::from_palette(palette, 0.).ink
                })
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_ellipsis()
                        .child(self.filter_label()),
                )
                .child(
                    div()
                        .debug_selector(|| "picker-filter-chevron".into())
                        .w(px(CHEVRON))
                        .h(px(CHEVRON))
                        .flex_none()
                        .children(crate::g2::st1::glyphs::svg("chevron-down").map(
                            |(key, data)| {
                                div()
                                    .debug_selector(|| "picker-filter-chevron-glyph".into())
                                    .child(Glyph {
                                        paint: Paint::Svg(key, data),
                                        side: CHEVRON,
                                        ink: palette.foreground.opacity(CHEVRON_INK),
                                    })
                            },
                        )),
                )
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                // Enter, Space and Down open the menu on their press in control_key; gpui's keyboard click on the release would reopen it after a pick.
                .on_click(cx.listener(|picker, event: &gpui::ClickEvent, window, cx| {
                    if !event.is_keyboard() {
                        picker.open_filter(window, cx);
                    }
                })),
        )
    }

    // A save row too narrow for the whole 300 px field, the trigger and the answers moves the trigger up, as v0.3.12 does.
    pub(super) fn filter_stacks(&self, window: &Window, cx: &gpui::App) -> bool {
        if !flea::ui_logic_pk1::saving(&self.req) || self.req.filters.is_empty() {
            return false;
        }
        // The buttons and label draw in the root's live typeface, never the window's default text style.
        let font = crate::list::Typeface::for_window(window, cx).font;
        let measure = |text: SharedString, size: f32| shaped_width(window, &font, text, size);
        let button = |text: SharedString| measure(text, TEXT) + 2. * (BUTTON_SIDE + FRAME);
        let catalog = &self.locale.catalog;
        let label: SharedString = catalog.tr("picker.browser.save_as").to_uppercase().into();
        let tracking = crate::g2::fb1::caption::tracking_for(catalog.language());
        let label =
            crate::g2::fb1::caption::measure(label, font.clone(), px(LABEL), px(tracking), window);
        let need = 2. * SIDE
            + button(catalog.tr("picker.browser.new_folder").into())
            + GAP
            + SPACER
            + GAP
            + label
            + GAP
            + FIELD
            + GAP
            + FILTER_WIDTH
            + GAP
            + button(self.labels.cancel.clone())
            + GAP
            + button(self.answer_label());
        need > f32::from(window.viewport_size().width)
    }
}

// The width one line of text shapes to in the given face, the implicitWidth v0.3.12 lays its rows out by.
pub(super) fn shaped_width(
    window: &Window,
    font: &gpui::Font,
    text: SharedString,
    size: f32,
) -> f32 {
    let run = gpui::TextRun {
        len: text.len(),
        font: font.clone(),
        color: crate::palette::Theme::default().foreground,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    f32::from(
        window
            .text_system()
            .shape_line(text, px(size), &[run], None)
            .width,
    )
}
