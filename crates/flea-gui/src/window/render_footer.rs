//! The footer follows the producer currently covering the pane.
use crate::state::Shell;
use gpui::Context;

impl Shell {
    pub(crate) fn sync_footer_owner(&mut self, cx: &mut Context<Self>) {
        let external_line = self
            .trash
            .as_ref()
            .filter(|host| host.side == self.focus_side && self.slots.covers[host.side].is_some())
            .map(|host| host.messages.clone());
        self.footer(cx).update(cx, |footer, cx| {
            if footer.external_line.as_ref().map(|line| line.entity_id())
                != external_line.as_ref().map(|line| line.entity_id())
            {
                footer.external_line = external_line;
                cx.notify();
            }
        });
    }
}
