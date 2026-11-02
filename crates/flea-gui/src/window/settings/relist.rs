//! Apply pending listing preferences once the pane can accept them.
use crate::state::Shell;
use gpui::{Context, Window};

impl Shell {
    pub(crate) fn settle_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for side in 0..2 {
            if !self.settings.relist[side] {
                continue;
            }
            let Some(host) = &mut self.panes[side] else {
                self.settings.relist[side] = false;
                self.settings.relist_fresh[side] = false;
                continue;
            };
            if host.stopped {
                self.settings.relist[side] = false;
                self.settings.relist_fresh[side] = false;
                continue;
            }
            if host.nav.busy(&host.pane) {
                continue;
            }
            let asked = if self.settings.relist_fresh[side] && host.grid_anchor_viewport().is_none()
            {
                host.nav.fresh_preferences(&mut host.pane)
            } else {
                host.reread_preferences()
            };
            if asked {
                self.settings.relist[side] = false;
                self.settings.relist_fresh[side] = false;
                self.run_navigation(side, window, cx);
            }
        }
    }
}
