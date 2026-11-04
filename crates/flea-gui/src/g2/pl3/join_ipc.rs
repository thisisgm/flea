//! Trash's focus IPC: trashFocusEmpty and trashFocusListing move the keyboard, so they run before the read-only readers.
use crate::state::Shell;
use flea::ipc::{
    registry,
    wire::{Reply, Request},
};
use gpui::{Context, Window};

impl Shell {
    /// Answers the Trash focus calls; None leaves the request to the read-only adapters.
    pub(crate) fn trash_focus_call(
        &mut self,
        request: &Request,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Reply> {
        let Request::Call {
            target,
            function,
            arguments,
        } = request
        else {
            return None;
        };
        if target != "flea" || !matches!(function.as_str(), "trashFocusEmpty" | "trashFocusListing")
        {
            return None;
        }
        if let Err(reply) = registry::validate(target, function, arguments) {
            return Some(reply);
        }
        let on = function == "trashFocusEmpty";
        let Some(host) = self.trash.as_ref() else {
            return Some(Reply::value("false"));
        };
        let pane = host.pane.clone();
        if !pane.read(cx).view().opened {
            return Some(Reply::value("false"));
        }
        pane.update(cx, |pane, cx| pane.focus_strip(on, cx));
        let focus = pane.read(cx).focus.clone();
        window.focus(&focus, cx);
        let on_strip = pane.read(cx).view().strip_focus;
        Some(Reply::value((on_strip == on).to_string()))
    }
}
