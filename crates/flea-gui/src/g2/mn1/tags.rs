//! Shift+T prepares the existing Tags flyout from the backend's captured selection state.
use crate::state::Shell;
use gpui::{Context, Window};

impl Shell {
    pub(crate) fn open_tags_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let side = self.focus_side;
        if self.locked_folder(side).is_some()
            || self.panes[side]
                .as_ref()
                .is_none_or(|host| host.pane.held.row(host.pane.cursor).is_none())
        {
            return;
        }
        self.open_flyout("tags", window, cx);
    }
}
