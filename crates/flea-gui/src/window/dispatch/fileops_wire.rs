//! The file operations family's replies: New Folder and Duplicate land, Trash clears the marks, Delete permanently opens its card.
use super::Order;
use crate::{
    navigation::history::select_match,
    pane::states::Kind,
    state::Shell,
    window::{input::ActionEvent, navigate::Step, startup::View},
};
use flea::{
    jsondoc::{self, Json},
    ui_logic_fb1::status::Hint,
    ui_logic_op1::{
        confirm::Source,
        lines,
        trash::{Review, Scope},
        Event, Reply,
    },
};
use gpui::{Context, Window};

impl Shell {
    // Sample input: {"t":"made","ok":true,"path":"/home/gm/New Folder"}; duplicated and the trash and delete replies share the same t field.
    /// A backend line this family answers; true when nobody else needs it.
    pub(crate) fn fileops_line(
        &mut self,
        side: usize,
        line: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Ok(document) = jsondoc::parse(line) else {
            return false;
        };
        let text = |key| document.get(key).and_then(Json::as_str).unwrap_or("");
        match text("t") {
            "linked" => {
                let count = |key| match document.get(key) {
                    Some(Json::Num(value)) => value.parse::<u64>().unwrap_or(0),
                    _ => 0,
                };
                let outcome = lines::Outcome {
                    ok: count("ok"),
                    failed: count("failed"),
                    skipped: count("skipped"),
                    note: text("note"),
                    ..Default::default()
                };
                let said = lines::linked_receipt(&self.locale.catalog, outcome);
                let error = (outcome.failed > 0 && outcome.ok == 0) || !outcome.note.is_empty();
                let hint = if outcome.ok > 0 {
                    Hint::TransientUndo
                } else {
                    Hint::None
                };
                if let Some(host) = self.panes[side].as_mut() {
                    host.status = said.clone().into();
                }
                self.status_messages(cx, |messages, now| {
                    messages.say(&said, error, "", hint, now);
                });
                let _ = self.step(side, Step::Refresh(None), window, cx);
                true
            }
            // PaneWire.qml onMade and onDuplicated: the line, then a re-read with the new item under the cursor.
            kind @ ("made" | "duplicated") => {
                let path = text("path").to_owned();
                if document.get("ok").and_then(Json::as_bool) != Some(true) || path.is_empty() {
                    return true;
                }
                let catalog = self.locale.catalog.clone();
                let preset = self.fileops_preset();
                let said = if kind == "made" {
                    lines::made(&catalog, &preset, &path)
                } else {
                    let leaf = flea::ui_logic_op1::leaf(&path);
                    let line = catalog.tr_args("jobs.done.duplicated", &[leaf]);
                    lines::with_undo(&catalog, &preset, line)
                };
                self.fileops_say(side, &said, false, cx);
                if kind == "made" {
                    self.operations.fileops.arrival[side] = Some(path.clone());
                }
                // Shell::step says its own refusal in the pane status, so its result is spent here.
                let _ = self.step(side, Step::Refresh(Some(path)), window, cx);
                true
            }
            // PaneWire.qml onTrashed clears the marks; the operations overlay says the line.
            "trashed" => {
                if let Some(host) = self.panes[side].as_mut() {
                    host.selected.clear();
                }
                false
            }
            "menuaction" => self.fileops_delete_reply(side, line, window, cx),
            _ => false,
        }
    }

    /// PaneWire.qml openRenameOnArrival: once the re-read lands, the new folder's own row takes the cursor and Rename runs on it.
    pub(crate) fn fileops_arrival(&mut self, side: usize, window: &mut Window, cx: &mut Context<Self>) {
        // Retained hidden panes finish backend replies without opening an editor on the visible pane.
        if side == 1 && self.view != View::Dual {
            self.operations.fileops.arrival[side] = None;
            return;
        }
        let Some(host) = self.panes[side].as_mut() else {
            return;
        };
        let arrival = &mut self.operations.fileops.arrival[side];
        if arrival.is_none() || host.pane.busy() || host.nav.pending_select.is_some() {
            // An ended listing never delivers the folder, so its editor is dropped rather than opened over a later one.
            if matches!(host.pane.state.kind, Kind::Error | Kind::Locked) && !host.pane.busy() {
                *arrival = None;
            }
            return;
        }
        let Some(target) = arrival.take() else {
            return;
        };
        let names = host.pane.held.rows.iter().map(|row| row.name.as_str());
        let Some(at) = select_match(names, &target, host.pane.path()) else {
            return;
        };
        host.pane.cursor = host.pane.held.start + at;
        let rename = ActionEvent::bare("rename", flea::keys::Context::Listing);
        self.focus_pane(side, cx);
        self.dispatch(Order::Action(rename), window, cx);
    }

    // The Delete permanently snapshot this family sent, then its review, which opens the permanent-delete card.
    fn fileops_delete_reply(
        &mut self,
        side: usize,
        line: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(Event::MenuAction(reply)) = flea::ui_logic_op1::parse(line) else {
            return false;
        };
        if self.operations.fileops.deleting.get(side).copied().flatten() != Some(reply.id) {
            return false;
        }
        match (reply.op.as_str(), reply.ok) {
            ("snapshot", true) => {
                let id = reply.id;
                let ask = format!("{{\"c\":\"menuaction\",\"op\":\"prepareDelete\",\"id\":{id}}}");
                self.fileops_send(side, ask, cx);
            }
            ("prepareDelete", true) => {
                self.operations.fileops.deleting[side] = None;
                let review = Review {
                    count: Some(reply.count),
                    bytes: reply.bytes,
                    all: false,
                    token: reply.token,
                    stale: false,
                };
                self.confirm_operation(side, review, Scope::Items, Source::Menu(reply.id), window, cx);
            }
            ("snapshot" | "prepareDelete", false) => self.fileops_delete_refused(side, &reply, cx),
            _ => return false,
        }
        true
    }

    // A refused review says why and closes the snapshot it was taken on.
    fn fileops_delete_refused(&mut self, side: usize, reply: &Reply, cx: &mut Context<Self>) {
        self.operations.fileops.deleting[side] = None;
        let close = format!("{{\"c\":\"menuaction\",\"op\":\"close\",\"id\":{}}}", reply.id);
        self.fileops_send(side, close, cx);
        let text = if reply.error.is_empty() {
            self.locale.catalog.tr("jobs.delete.failed")
        } else {
            reply.error.clone()
        };
        self.fileops_say(side, &text, true, cx);
    }
}
