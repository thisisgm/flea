//! The operations overlay slot: op1's Overlay over the panes under its scrim, built on first open.
use crate::{g2::op1::host::Overlay, state::Shell};
use gpui::{App, Context, Window};

/// A card or the Jobs list is showing: it holds the keyboard, and the list underneath draws grey.
pub(crate) fn card_showing(card: &Overlay, cx: &App) -> bool {
    card.showing(cx)
}

impl Shell {
    /// An overlay entity can outlive its card, so only a card still showing takes the keyboard from the list.
    pub(crate) fn card_holds_keyboard(&self, cx: &App) -> bool {
        self.settings.opened
            || self.settings.sheet.is_some()
            || self.output_showing(cx)
            || self.network_holds_keyboard(cx)
            || self.rename.bulk.is_some()
            || self.properties.is_some()
            || self.run_card.is_some()
            || self.open_with.is_some()
            || self.unlock_holds_keyboard(cx)
            || self
                .overlay
                .as_ref()
                .is_some_and(|overlay| overlay.read_with(cx, |card, _| card_showing(card, cx)))
    }

    /// Opens the Copy To card on the overlay, building it on the first open; needs no backend reply.
    pub(crate) fn open_overlay_copy_to(
        &mut self,
        moving: bool,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A retained operation session keeps its backend owner when another pane opens a card.
        if self.overlay.as_ref().is_some_and(|overlay| {
            overlay.read_with(cx, |overlay, _| overlay.owner != Some(self.focus_side))
        }) {
            self.close_overlay(window, cx);
        }
        self.copy_to_operation(self.focus_side, 0, moving, path, window, cx);
        self.focus_operation_mount(window, cx);
        if let Some(overlay) = &self.overlay {
            overlay.update(cx, |overlay, cx| overlay.sync_focus(window, cx));
        }
        cx.notify();
    }

    /// A backend that cannot complete again loses only its own activity and operation presentation.
    pub(crate) fn retire_status_owner(
        &mut self,
        side: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(footer) = &self.surfaces[0].footer {
            let line = footer.read_with(cx, |footer, _| footer.line.clone());
            line.update(cx, |line, cx| {
                line.update(cx, |messages, now| {
                    messages.set_activity(side as u64, "", 0, false, false, now)
                });
            });
        }
        if self
            .overlay
            .as_ref()
            .is_some_and(|overlay| overlay.read_with(cx, |overlay, _| overlay.owner == Some(side)))
        {
            self.close_overlay(window, cx);
        }
    }

    /// Closes whatever card the overlay shows; the window draws its own frame again.
    pub(crate) fn close_overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(overlay) = self.overlay.take() {
            overlay.update(cx, |overlay, cx| {
                overlay.model.key(&overlay.catalog, "escape", false);
                overlay.schedule(cx);
                overlay.release_focus(window, cx);
                cx.notify();
            });
        }
        cx.notify();
    }
}
