//! Refresh every retained card palette from the same presentation roles.
use crate::{
    g2::{file_ops::palette::CardPalette, quick_look_overlay::Look},
    palette::Theme,
    state::Shell,
};
use gpui::{Context, Window};

impl Shell {
    pub(crate) fn restyle_frames(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        let palette = CardPalette::from_theme(&window.text_style(), theme, self.corner);
        if let Some(view) = &self.overlay {
            view.update(cx, |view, cx| {
                view.palette = palette;
                cx.notify();
            });
        }
        if let Some(view) = &self.output.view {
            let face = self.active_typeface();
            view.update(cx, |view, cx| {
                view.palette = palette;
                view.restyle_printers(&face, window);
                cx.notify();
            });
        }
        if let Some(network) = self.rail.as_ref().and_then(|join| join.network.as_ref()) {
            network
                .card
                .update(cx, |view, cx| view.set_palette(palette, cx));
            network
                .shares
                .update(cx, |view, cx| view.set_palette(palette, cx));
        }
        self.restyle_quick_look(window, cx);
    }

    // Opening Quick Look needs only its own card restyled, never the other retained cards.
    pub(crate) fn restyle_quick_look(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(host) = &self.quick_look else {
            return;
        };
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        let look = Look::of_text(&window.text_style(), theme).with_frames(
            crate::presentation::frames::Frames::from_palette(
                &crate::list::Palette::inherited(&window.text_style(), theme),
                self.corner,
            ),
        );
        host.view.update(cx, |view, cx| view.set_look(look, cx));
    }
}
