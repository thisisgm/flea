//! Like the shipped pane FocusScope, network overlays return to the remembered list or rail child.
use crate::{rail::Mode, state::Shell};
use flea::keys::Context as Keys;
use gpui::{Context, Window};

impl Shell {
    pub(crate) fn network_remember_focus(&mut self) {
        let origin = (self.focus_side, self.keyboard.context == Keys::Rail);
        if let Some(network) = self.network() {
            network.return_focus.get_or_insert(origin);
        }
    }

    pub(crate) fn network_restore_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let remembered = self
            .network()
            .and_then(|network| network.return_focus.take());
        // A later listing focus takes precedence over the overlay's older rail return target.
        let rail = remembered == Some((self.focus_side, true))
            && !self.focus.is_focused(window)
            && self
                .rail
                .as_ref()
                .is_some_and(|join| join.host.read(cx).rail.mode() != Mode::Hidden);
        self.rail_focus(rail, cx);
        self.keyboard.context = if rail { Keys::Rail } else { Keys::Listing };
        window.focus(
            if rail {
                &self.dual_focus.rail
            } else {
                &self.focus
            },
            cx,
        );
        cx.notify();
    }
}
