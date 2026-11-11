//! A folder's remembered sort belongs to the folder, so every other pane open on it shows the same order.
use crate::{
    navigation::{sort::Request, Command},
    state::Shell,
};
use gpui::{Context, Window};

/// The folders a remembered-sort write names: a chosen order or a forgotten one.
// Sample input: Command::Persist(r#"{"folderSorts":{"/home/u/b":{"key":"date","reverse":true}}}"#)
pub(crate) fn sorted_folders(commands: &[Command]) -> Vec<String> {
    commands
        .iter()
        .filter_map(|command| match command {
            Command::Persist(text) => flea::jsondoc::parse(text).ok(),
            _ => None,
        })
        .filter_map(|patch| {
            let sorts = patch.get("folderSorts")?.as_object()?;
            Some(
                sorts
                    .iter()
                    .map(|(path, _)| path.clone())
                    .collect::<Vec<_>>(),
            )
        })
        .flatten()
        .collect()
}

impl Shell {
    // The other pane takes the folder's order as a restore, so it re-sorts without writing a second choice.
    pub(crate) fn follow_folder_sort(
        &mut self,
        from: usize,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for side in (0..self.panes.len()).filter(|side| *side != from) {
            let Some(host) = self.panes[side].as_mut() else {
                continue;
            };
            let order = host.nav.folders.borrow().order_for(path);
            if !host.nav.is_folder(&host.pane) {
                continue;
            }
            // A listing still out on the folder was asked in the old order, so the order waits for it to land.
            if let Some(pending) = host.pane.swap.pending.as_ref().filter(|_| host.pane.busy()) {
                if pending.path == path {
                    host.nav.pending_sort = Some(Request::Restore(order));
                }
                continue;
            }
            if host.path != path {
                continue;
            }
            if host
                .nav
                .restore_order(&mut host.pane, order, &mut host.selected)
                .unwrap_or(false)
            {
                self.run_navigation(side, window, cx);
            }
        }
    }
}
