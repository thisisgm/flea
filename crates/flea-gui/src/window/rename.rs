//! The inline rename editor and the bulk rename card's opening: Ops.js startRename and commitRename, Pane.qml openBulkRename and PaneWire.qml's rename replies.
use crate::{path::entry::Entry, state::Shell};
use flea::{json::escape, ui_logic_file_ops::lines, ui_logic_status_feedback::status::Hint};
use gpui::{ClipboardItem, Context, KeyDownEvent, Window};

#[path = "rename_bulk.rs"]
mod bulk;
#[path = "rename_field.rs"]
pub(crate) mod field;
#[path = "rename_input.rs"]
mod input;
#[path = "rename_ipc.rs"]
pub(crate) mod ipc;
#[path = "rename_menu.rs"]
pub(crate) mod menu;
#[path = "rename_place.rs"]
mod place;
#[path = "rename_replies.rs"]
mod replies;
#[path = "rename_slow.rs"]
pub(crate) mod slow;
#[path = "rename_target.rs"]
mod target;

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
    pub(crate) ime: crate::g2::file_ops::dialog_editor::DialogEditor,
    pub edit: Option<Edit>,
    pub(crate) neighbour: Option<(String, String)>,
    pub bulk: Option<bulk::Open>,
    /// The side whose selection's paths are out for the bulk card.
    pub paths: Option<usize>,
    /// Each side whose bulk request is out, so its done line is this owner's; every pane has its own wire.
    pub sent: [bool; 2],
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
        // A name that is not UTF-8 is edited as its lossy spelling; the source keeps its exact bytes.
        let name = flea::bytename::display(leaf(&row.name)).into_owned();
        let mut entry = Entry::default();
        entry.begin(&name);
        // The stem alone, which is the part a rename usually changes.
        let cut = name.rfind('.').filter(|&at| at > 0).unwrap_or(name.len());
        entry.move_to(0);
        entry.select_to(cut);
        self.rename.neighbour = None;
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
        self.rename.neighbour = None;
        if let Some(edit) = self.rename.edit.take() {
            if edit.menu != 0 {
                self.menu_send(
                    edit.side,
                    format!(r#"{{"c":"menuaction","op":"close","id":{}}}"#, edit.menu),
                    cx,
                );
            }
            cx.notify();
        }
    }

    /// The list scrolled the edited row wholly out of view: the edit is dropped uncommitted, and a request out keeps its reply's owner.
    pub(super) fn rename_scrolled_out(
        &mut self,
        side: usize,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.renaming_row(side) != Some(index) {
            return;
        }
        match self.rename.edit.as_mut() {
            Some(edit) if edit.pending.is_some() => {
                edit.entry.close();
            }
            _ => self.rename.edit = None,
        }
        self.rename_sync_gate(window, cx);
        cx.notify();
    }

    /// The editor's keys while it holds the keyboard; typed text arrives through the input handler.
    pub(crate) fn rename_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.menu.text.is_some() {
            return false;
        }
        if self.rename.bulk.is_some() {
            self.bulk_rename_key(event, window, cx);
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
        if edit.pending.is_none() && entry.ime_key(&stroke.key) {
            cx.notify();
            cx.stop_propagation();
            return true;
        }
        match stroke.key.as_str() {
            // RenameField.qml abandon(): a write already out cannot be abandoned, so the editor keeps its pending name.
            "escape" if edit.pending.is_some() => {
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
            "z" if command => entry.undo(),
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

    /// The band a refused name draws under its row: the caption line and one gap, RenameField.qml's errorHeight.
    pub(crate) fn rename_band(&self, side: usize, window: &Window) -> f32 {
        let Some(edit) = self
            .rename
            .edit
            .as_ref()
            .filter(|edit| edit.side == side && !edit.error.is_empty())
        else {
            return 0.;
        };
        if self.renaming_row(side).is_none() {
            return 0.;
        }
        let Some(host) = self.panes[side].as_ref() else {
            return 0.;
        };
        let typeface = host.list.borrow().typeface.clone();
        let gap = crate::list::Metrics::for_typeface(&typeface).gap;
        let caption =
            field::caption_line(window, &edit.error, typeface.font.clone(), typeface.caption);
        f32::from(caption) + gap
    }

    /// The Grow every list frame of this side takes this render: the refusal's band under the edited row, none in Grid.
    pub(crate) fn frame_grow(&self, side: usize, band: f32) -> crate::list::pane::Grow {
        let Some(host) = self.panes[side].as_ref() else {
            return Default::default();
        };
        if self.shown_view() == crate::window::startup::View::Grid {
            return Default::default();
        }
        crate::list::pane::Grow::of_row(self.renaming_row(side), band, host.filter.shown.as_deref())
    }

    /// The row the editor stands over, when it is this side's drawn listing.
    pub(crate) fn renaming_row(&self, side: usize) -> Option<usize> {
        let edit = self.rename.edit.as_ref()?;
        let host = self.panes[side].as_ref()?;
        if edit.side != side || !edit.entry.editing || edit.listing != host.pane.held.listing {
            return None;
        }
        if self.rename.neighbour.is_some() {
            return self.rename_neighbour_row(side).map(|(_, index, _)| index);
        }
        Some(edit.index)
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

#[cfg(test)]
#[path = "../g2/file_ops/i50_dialog_tests.rs"]
mod i50_dialog_tests;
