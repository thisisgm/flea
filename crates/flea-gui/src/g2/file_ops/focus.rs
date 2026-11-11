//! The active overlay mount releases its keyboard before a queued modal acquires it.
use crate::state::Shell;
use gpui::{Context, Entity, Window};

impl Shell {
    pub(crate) fn focus_operation_mount(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let next = self
            .overlay
            .as_ref()
            .filter(|view| Entity::read(view, cx).modal(cx))
            .cloned();
        if self.operations.focus_owner == next {
            return;
        }
        if let Some(previous) = self.operations.focus_owner.take() {
            previous.update(cx, |overlay, cx| overlay.release_mount_focus(window, cx));
        }
        if let Some(view) = &next {
            view.update(cx, |overlay, cx| overlay.acquire_mount_focus(window, cx));
        }
        self.operations.focus_owner = next;
    }
}
