//! Captured menu choices and status publication retain their operation session owner.
use super::*;

impl Shell {
    // The folder's background New File row opens its card; the menu's own check finds the row claimed and nothing refuses.
    /// Opens the New File card for a background row, refusing Recent and unbacked panes before any request goes out
    fn open_new_file(
        &mut self,
        chosen: &crate::g2::mn1::join::Chosen,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let side = chosen.captured.side;
        if !chosen.captured.background {
            return;
        }
        // Pane.qml act: Recent is a history, so New File refuses there before any card opens, with the check New Folder's key uses.
        if self.recent_refuses(side, "menu.refusal.recent_new_file", cx) {
            chosen.claim();
            return;
        }
        if self
            .panes
            .get(side)
            .and_then(Option::as_ref)
            .is_none_or(|host| host.backend.is_none())
        {
            return;
        }
        let Some(view) = self.operation_overlay(side, window, cx) else {
            return;
        };
        if !chosen.claim() {
            return;
        }
        let folder = chosen.captured.path.clone();
        let Some(id) = self.fileops_creation_requested(side, "newFile", &folder, cx) else {
            return;
        };
        view.update(cx, |overlay, cx| overlay.new_file(id, &folder, None, cx));
        self.show_operation(&view, cx);
        self.flush_operations(side, cx);
    }

    /// Subscribes the shell to chosen menu actions and to operation status updates, once per shell
    pub(crate) fn observe_operations(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.operations.chosen_subscription.is_some() {
            return;
        }
        let entity = cx.entity();
        self.operations.chosen_subscription = Some(cx.subscribe_in(
            &entity,
            window,
            |shell, _, chosen: &crate::g2::mn1::join::Chosen, window, cx| {
                if shell.archive_chosen(chosen, window, cx) {
                    return;
                }
                if chosen.action == "newFile" {
                    shell.open_new_file(chosen, window, cx);
                    return;
                }
                let side = chosen.captured.side;
                if !matches!(chosen.action.as_str(), "copyTo" | "moveTo")
                    || !chosen.captured.ready
                    || chosen.captured.background
                    || chosen.paths.is_empty()
                    || shell
                        .panes
                        .get(side)
                        .and_then(Option::as_ref)
                        .is_none_or(|host| host.backend.is_none())
                {
                    return;
                }
                let Some(view) = shell.operation_overlay(side, window, cx) else {
                    return;
                };
                if !chosen.claim() {
                    return;
                }
                shell.operations.origins[side] = Some(chosen.clone());
                view.update(cx, |overlay, cx| {
                    overlay.model.held_listing = chosen.captured.listing as u64;
                    overlay.copy_to(
                        chosen.captured.id,
                        chosen.action == "moveTo",
                        &chosen.captured.path,
                        cx,
                    );
                });
                shell.show_operation(&view, cx);
                shell.flush_operations(side, cx);
            },
        ));
        self.operations.status_subscription = Some(cx.subscribe_in(
            &entity,
            window,
            |shell, _, update: &OperationUpdate, _, cx| {
                if let Some(said) = update.said.last() {
                    let catalog = shell.locale.catalog.clone();
                    shell.status_messages(cx, |messages, now| {
                        messages.say_receipt(&catalog, &said.text, said.error, &said.detail, now);
                    });
                }
            },
        ));
    }
}
