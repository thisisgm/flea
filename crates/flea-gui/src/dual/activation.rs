//! Keep the complete current layout until a new secondary has a complete surface to show.
use crate::{
    pane::states::Kind,
    state::Shell,
    window::{lifecycle::Phase, startup::View},
};
use gpui::{Context, Window};

impl Shell {
    pub(crate) fn request_dual(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dual_pending = true;
        // A fresh second pane owes no jump answers its earlier backend left out.
        self.chrome.jumps[1] = 0;
        self.start_pane(1, window, cx);
        self.settle_dual(window, cx);
    }

    pub(crate) fn settle_dual(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.dual_pending || self.lifecycle.phase() != Phase::Open {
            return;
        }
        let Some(host) = &self.panes[1] else {
            return;
        };
        // A stopped secondary shows its error state, which is a completed surface.
        if host.pane.held.listing == 0
            && !host.stopped
            && !host.pane.swap.fell_back
            && matches!(host.pane.state.kind, Kind::Loading)
        {
            return;
        }
        self.dual_pending = false;
        self.view = View::Dual;
        self.focus_side = self.dual_focus.last_side;
        // WindowBody.focusPane: the focused side's list takes the keys and the rail lets go.
        self.keyboard.context = flea::keys::Context::Listing;
        self.focus_rail(false, cx);
        window.focus(&self.focus, cx);
        self.remember_dual();
        cx.notify();
    }
}
