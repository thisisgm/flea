//! Only an accepted tab change closes the pane's Quick Look, filter and search walk; a refused one keeps them.
use crate::state::Shell;
use gpui::{Context, Window};

impl Shell {
    pub(crate) fn reset_tab_overlays(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for side in 0..2 {
            let notice = &mut self.strip.notice[side];
            let preview = std::mem::take(&mut notice.close_preview);
            let overlay = std::mem::take(&mut notice.drop_overlay);
            if preview
                && self
                    .quick_look
                    .as_ref()
                    .is_some_and(|host| host.side == side)
            {
                self.quick_look_tab_switch(window, cx);
            }
            if overlay {
                self.filter_update(
                    side,
                    |filter, pane| {
                        filter.close(pane);
                        None
                    },
                    cx,
                );
                self.search_leave(side, window, cx);
            }
        }
    }
}
