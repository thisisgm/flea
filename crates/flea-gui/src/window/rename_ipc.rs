//! The rename readers Ipc.qml served: renamingIndex, renameEditorLive, renameEditorText, renameState and bulkRenameState.
use super::Shell;
use crate::{
    g2::open_with_properties::shell::Block,
    ipc::{rows::centre_of, Adapter},
};
use flea::{ipc::wire::Reply, json::escape};

pub(crate) struct Reader<'a> {
    pub shell: &'a Shell,
}

impl Reader<'_> {
    fn state(&self) -> String {
        let shell = self.shell;
        let side = shell.focus_side;
        let edit = shell
            .rename
            .edit
            .as_ref()
            .filter(|edit| edit.side == side && shell.renaming_row(side).is_some());
        let host = shell.panes[side].as_ref();
        let cursor = host.map_or(0, |host| host.pane.cursor);
        let cursor_name = host
            .and_then(|host| host.pane.held.row(cursor))
            .map_or("", |row| row.name.as_str());
        let (row, area) = host.map_or((0., 0.), |host| {
            let list = host.list.borrow();
            (
                list.drawn.metrics.row,
                f32::from(list.drawn.bounds.size.height),
            )
        });
        // RenameField.qml's errorHeight: the caption band every view draws, Grid included.
        let band = shell.bands[side];
        let top = host.map_or(0., |host| {
            f32::from(host.list.borrow().drawn.bounds.origin.y)
        });
        let frame = edit.and_then(|edit| edit.line.frame);
        let error = edit.is_some_and(|edit| !edit.error.is_empty());
        let (field, editor_top, centre) = frame.map_or((0., 0., String::new()), |frame| {
            let (x, y) = (f32::from(frame.origin.x), f32::from(frame.origin.y));
            let (w, h) = (f32::from(frame.size.width), f32::from(frame.size.height));
            (h, y - top, centre_of(x, y, w, h))
        });
        let error_height = if error { band } else { 0. };
        let selected = edit.map_or("", |edit| &edit.entry.text[edit.entry.selected.clone()]);
        let listing_state = host.map_or("", |host| host.pane.state.kind.name());
        format!(
            "{{\"index\":{},\"pending\":{},\"error\":\"{}\",\"text\":\"{}\",\"cursor\":{cursor},\"cursorName\":\"{}\",\"loading\":{},\"listingState\":\"{listing_state}\",\"focused\":{},\"rowHeight\":{},\"normalRowHeight\":{row},\"fieldHeight\":{field},\"errorHeight\":{error_height},\"editorTop\":{editor_top},\"editorBottom\":{},\"viewportHeight\":{area},\"selectedText\":\"{}\",\"centre\":\"{centre}\"}}",
            shell.renaming_row(side).map_or(-1, |row| row as i64),
            edit.is_some_and(|edit| edit.pending.is_some()),
            escape(edit.map_or("", |edit| &edit.error)),
            escape(edit.map_or("", |edit| &edit.entry.text)),
            escape(cursor_name),
            host.is_some_and(|host| host.pane.busy()),
            edit.is_some_and(|edit| edit.entry.editing && edit.pending.is_none()),
            if edit.is_some() { row } else { 0. },
            editor_top + field,
            escape(selected),
        )
    }

    fn bulk(&self) -> String {
        let Some(open) = self.shell.rename.bulk.as_ref() else {
            return "{\"opened\":false}".into();
        };
        let card = open.card.card(&self.shell.locale.catalog);
        let footer = card.body.iter().find_map(|block| match block {
            Block::Note { text, .. } => Some(text.as_str()),
            _ => None,
        });
        let rename = card.buttons.last();
        format!(
            "{{\"opened\":true,\"footer\":\"{}\",\"rename\":\"{}\",\"canRename\":{},\"count\":{},\"focus\":\"{:?}\"}}",
            escape(footer.unwrap_or("")),
            escape(rename.map_or("", |button| button.label.as_str())),
            rename.is_some_and(|button| button.enabled),
            open.card.names.len(),
            open.card.focus,
        )
    }
}

impl Adapter for Reader<'_> {
    fn call(&self, function: &str, _: &[String]) -> Option<Reply> {
        let shell = self.shell;
        let side = shell.focus_side;
        let live = shell
            .rename
            .edit
            .as_ref()
            .filter(|edit| shell.renaming_row(side).is_some() && edit.entry.editing);
        let value = match function {
            "renamingIndex" => shell
                .renaming_row(side)
                .map_or(-1, |row| row as i64)
                .to_string(),
            "renameEditorLive" => live.is_some().to_string(),
            "renameEditorText" => live.map_or(String::new(), |edit| edit.entry.text.clone()),
            "renameState" => self.state(),
            "bulkRenameState" => self.bulk(),
            _ => return None,
        };
        Some(Reply::value(value))
    }
}
