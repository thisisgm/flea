//! The inline rename editor and the bulk rename card's opening: Ops.js startRename and commitRename, Pane.qml openBulkRename and PaneWire.qml's rename replies.
use crate::{path::entry::Entry, state::Shell};
use flea::{json::escape, ui_logic_fb1::status::Hint, ui_logic_op1::lines};
use gpui::{ClipboardItem, Context, KeyDownEvent, Window};

#[path = "rename_bulk.rs"]
mod bulk;
#[path = "rename_field.rs"]
pub(crate) mod field;
#[path = "rename_input.rs"]
mod input;
#[path = "rename_ipc.rs"]
pub(crate) mod ipc;
#[path = "rename_replies.rs"]
mod replies;
#[path = "rename_slow.rs"]
pub(crate) mod slow;

/// One row's name as an editor: the row the request named, the source it renames and the draft.
pub(crate) struct Edit {
    pub side: usize,
    pub index: usize,
    pub listing: usize,
    pub source: String,
    pub name: String,
    pub menu: u64,
    pub entry: Entry,
    pub error: String,
    /// The request out: the destination its reply closes.
    pub pending: Option<String>,
    pub line: field::Shaped,
}

#[derive(Default)]
pub(crate) struct Join {
    pub edit: Option<Edit>,
    pub bulk: Option<bulk::Open>,
    /// The side whose selection's paths are out for the bulk card.
    pub paths: Option<usize>,
    /// The side whose bulk request is out, so its done line is this owner's.
    pub sent: Option<usize>,
    /// The last lone tap on a name, for the slow click.
    pub slow: Option<slow::Slow>,
}

impl Shell {
    /// F2, the mac Return, r and the file menu's Rename: one row opens the editor, several open the bulk card.
    pub(crate) fn rename_open(
        &mut self,
        side: usize,
        menu: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(host) = self.panes[side].as_ref() else {
            return false;
        };
        // Ops.js startRename: a request still out keeps its editor.
        if self
            .rename
            .edit
            .as_ref()
            .is_some_and(|edit| edit.pending.is_some())
        {
            return true;
        }
        if host.selected.len() > 1 {
            return self.bulk_rename_ask(side, cx);
        }
        let index = host.pane.cursor;
        let Some(row) = host.pane.held.row(index).filter(|_| !host.pane.busy()) else {
            let said = lines::no_target(&self.locale.catalog, index as i64, false);
            self.rename_say(said, false, cx);
            return true;
        };
        let source = join(host.pane.path(), &row.name);
        let name = leaf(&row.name).to_owned();
        let mut entry = Entry::default();
        entry.editing = true;
        entry.replace(None, &name);
        // The stem alone, which is the part a rename usually changes.
        let cut = name.rfind('.').filter(|&at| at > 0).unwrap_or(name.len());
        entry.move_to(0);
        entry.select_to(cut);
        self.rename.edit = Some(Edit {
            side,
            index,
            listing: host.pane.held.listing,
            source,
            name,
            menu,
            entry,
            error: String::new(),
            pending: None,
            line: Default::default(),
        });
        self.focus_pane(side, cx);
        window.focus(&self.focus, cx);
        cx.notify();
        true
    }

    /// Enter and an outside click: an unchanged name abandons, a refused one stays open with its reason.
    pub(crate) fn rename_commit(&mut self, cx: &mut Context<Self>) {
        let catalog = self.locale.catalog.clone();
        let Some(edit) = self.rename.edit.as_mut() else {
            return;
        };
        if edit.pending.is_some() {
            return;
        }
        let next = edit.entry.text.trim().to_owned();
        if edit.entry.text == edit.name || next == edit.name {
            self.rename_abandon(cx);
            return;
        }
        let side = edit.side;
        let live = self.panes[side].as_ref().is_some_and(|host| {
            host.pane.held.listing == edit.listing && host.pane.held.row(edit.index).is_some()
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
        let line = format!(
            r#"{{"c":"rename","path":"{}","to":"{}","menuId":{}}}"#,
            escape(&edit.source),
            escape(&next),
            edit.menu
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

    /// Escape: the draft goes and the row shows its name again.
    pub(crate) fn rename_abandon(&mut self, cx: &mut Context<Self>) {
        if self.rename.edit.take().is_some() {
            cx.notify();
        }
    }

    /// The editor's keys while it holds the keyboard; typed text arrives through the input handler.
    pub(crate) fn rename_key(
        &mut self,
        event: &KeyDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.rename.bulk.is_some() {
            self.bulk_rename_key(event, cx);
            cx.stop_propagation();
            return true;
        }
        // An editor whose row left the drawn listing lets go of the keys; a request out keeps its reply's owner.
        if let Some(edit) = self.rename.edit.as_ref() {
            if edit.entry.editing && self.renaming_row(edit.side).is_none() {
                if edit.pending.is_none() {
                    self.rename.edit = None;
                } else if let Some(edit) = self.rename.edit.as_mut() {
                    edit.entry.close();
                }
            }
        }
        let Some(edit) = self.rename.edit.as_mut().filter(|edit| edit.entry.editing) else {
            return false;
        };
        let stroke = &event.keystroke;
        let held = stroke.modifiers;
        let command = held.control || held.platform;
        let entry = &mut edit.entry;
        match stroke.key.as_str() {
            // SlowOp.js: a request out keeps its reply's owner, so Escape only hides the editor.
            "escape" if edit.pending.is_some() => {
                entry.close();
                cx.notify();
                cx.stop_propagation();
                return true;
            }
            "escape" => {
                self.rename_abandon(cx);
                cx.stop_propagation();
                return true;
            }
            "enter" if entry.marked.is_none() => {
                self.rename_commit(cx);
                cx.stop_propagation();
                return true;
            }
            _ if edit.pending.is_some() => {}
            "backspace" | "delete" => entry.erase(stroke.key == "delete"),
            key @ ("left" | "right" | "home" | "end") => {
                let to = match key {
                    "left" if !held.shift && !entry.selected.is_empty() => entry.selected.start,
                    "right" if !held.shift && !entry.selected.is_empty() => entry.selected.end,
                    "left" => entry.previous(entry.cursor()),
                    "right" => entry.next(entry.cursor()),
                    "home" => 0,
                    _ => entry.text.len(),
                };
                if held.shift {
                    entry.select_to(to);
                } else {
                    entry.move_to(to);
                }
            }
            "a" if command => entry.select_all(),
            "c" | "x" if command && !entry.selected.is_empty() => {
                cx.write_to_clipboard(ClipboardItem::new_string(
                    entry.text[entry.selected.clone()].to_owned(),
                ));
                if stroke.key == "x" {
                    entry.replace(None, "");
                }
            }
            "v" if command => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    let text: String = text.chars().filter(|ch| !ch.is_control()).collect();
                    entry.replace(None, &text);
                }
            }
            // Typed text reaches the input handler only while the key goes on unstopped.
            _ if stroke.key_char.is_some() && !command => return true,
            _ => {}
        }
        cx.stop_propagation();
        cx.notify();
        true
    }

    /// A line in the footer's message service, in the error role for a refusal.
    pub(crate) fn rename_say(&mut self, text: String, error: bool, cx: &mut Context<Self>) {
        self.status_messages(cx, |messages, now| {
            messages.say(&text, error, "", Hint::None, now)
        });
        cx.notify();
    }

    /// The navigator's gate follows the editor: an open editor or a request out holds a sort and a re-read until it ends.
    pub(crate) fn rename_sync_gate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for side in 0..2 {
            let Some(host) = self.panes[side].as_ref() else {
                continue;
            };
            let ours = self.rename.edit.as_ref().filter(|edit| edit.side == side);
            let editing = ours.is_some_and(|edit| edit.entry.editing);
            let pending = ours.is_some_and(|edit| edit.pending.is_some());
            let gate = host.nav.gate;
            if gate.editing != editing || gate.rename_pending != pending {
                let gate = crate::navigation::watch::Gate {
                    editing,
                    rename_pending: pending,
                    ..gate
                };
                self.set_gate(side, gate, window, cx);
            }
        }
    }

    /// The row the editor stands over, when it is this side's drawn listing.
    pub(crate) fn renaming_row(&self, side: usize) -> Option<usize> {
        let edit = self.rename.edit.as_ref()?;
        let host = self.panes[side].as_ref()?;
        (edit.side == side && edit.entry.editing && edit.listing == host.pane.held.listing)
            .then_some(edit.index)
    }
}

fn join(folder: &str, name: &str) -> String {
    if name.starts_with('/') {
        return name.to_owned();
    }
    format!("{}/{name}", folder.trim_end_matches('/'))
}

fn leaf(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn parent(path: &str) -> &str {
    match path.rfind('/') {
        Some(0) => "/",
        Some(at) => &path[..at],
        None => "",
    }
}
