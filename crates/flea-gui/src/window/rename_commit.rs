use super::Shell;
use flea::{json::escape, ui_logic_file_ops::lines};
use gpui::Context;

impl Shell {
    /// Enter and an outside click: an unchanged name abandons, a refused one stays open with its reason.
    pub(crate) fn rename_commit(&mut self, cx: &mut Context<Self>) {
        let neighbour_live = self.rename.neighbour.is_some()
            && self
                .rename_neighbour_row(self.rename.edit.as_ref().map_or(0, |edit| edit.side))
                .is_some();
        let catalog = self.locale.catalog.clone();
        let Some(edit) = self.rename.edit.as_mut() else {
            return;
        };
        if edit.pending.is_some() || edit.entry.marked.is_some() {
            return;
        }
        edit.line.end_selection_drag();
        let next = edit.entry.text.trim().to_owned();
        if edit.entry.text == edit.name || next == edit.name {
            self.rename_abandon(cx);
            return;
        }
        let side = edit.side;
        let live = self.panes[side].as_ref().is_some_and(|host| {
            host.pane.held.listing == edit.listing
                && (neighbour_live
                    || (self.rename.neighbour.is_none()
                        && host.pane.held.row(edit.index).is_some()))
        });
        if !live {
            edit.error = lines::rename_changed(&catalog);
            cx.notify();
            return;
        }
        if let Some(refusal) = lines::rename_refusal(&catalog, &next) {
            edit.error = refusal;
            cx.notify();
            return;
        }
        edit.error.clear();
        let identity = edit
            .identity
            .as_ref()
            .filter(|_| edit.menu == 0)
            .map_or_else(String::new, |identity| {
                format!(r#","source":"{}""#, escape(identity))
            });
        let line = format!(
            r#"{{"c":"rename","path":"{}","to":"{}","menuId":{}{}}}"#,
            escape(&edit.source),
            escape(&next),
            edit.menu,
            identity
        );
        edit.pending = Some(lines::rename_destination(&edit.source, &next));
        let sent = self.panes[side]
            .as_ref()
            .and_then(|host| host.backend.as_ref())
            .map(|backend| backend.operation(line));
        if !matches!(sent, Some(Ok(()))) {
            self.rename_refused(String::new(), cx);
        }
        cx.notify();
    }
}
