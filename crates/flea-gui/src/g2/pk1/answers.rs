//! The existing answer row gains its leading actions without changing the IME-backed Filename.
use super::Picker;
use crate::list::Palette;
use gpui::{div, prelude::*, px, Context, Div, MouseButton, Window};

impl Picker {
    pub(super) fn answer_leading(
        &mut self,
        palette: &Palette,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut row = div().flex_1().min_w_0().flex().items_center().gap(px(9.));
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
        }
        row = row.children(self.filter.clone());
        if !self.req.filters.is_empty() {
            let focused = self.chrome.filter.is_focused(window);
            row = row.child(
                self.chrome
                    .probe
                    .attach(div().relative(), "picker-filter")
                    .id("picker-filter")
                    .debug_selector(|| "picker-filter".into())
                    .track_focus(&self.chrome.filter)
                    .min_w_0()
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
                    .child(self.filter_label())
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(|picker, _, window, cx| picker.open_filter(window, cx))),
            );
        }
        row
    }
}
