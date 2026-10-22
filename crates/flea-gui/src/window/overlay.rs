//! The operations overlay slot: op1's Overlay over the panes under its scrim, built on first open.
use crate::{g2::op1::host::Overlay, palette::Theme, state::Shell};
use gpui::{AppContext, Context, Window};

impl Shell {
    /// Opens the Copy To card on the overlay, building it on the first open; needs no backend reply.
    pub(crate) fn open_overlay_copy_to(
        &mut self,
        moving: bool,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        let palette = crate::g2::op1::palette::CardPalette::from_theme(
            &window.text_style(),
            theme,
            self.corner,
        );
        let catalog = self.locale.catalog.clone();
        let overlay = self
            .overlay
            .get_or_insert_with(|| cx.new(|_| Overlay::new(catalog, palette, "default")))
            .clone();
        overlay.update(cx, |overlay, cx| {
            overlay.copy_to(0, moving, path, cx);
            overlay.sync_focus(window, cx);
        });
        cx.notify();
    }

    /// Closes whatever card the overlay shows; the window draws its own frame again.
    pub(crate) fn close_overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(overlay) = self.overlay.take() {
            overlay.update(cx, |overlay, cx| overlay.release_focus(window, cx));
        }
        cx.notify();
    }
}
