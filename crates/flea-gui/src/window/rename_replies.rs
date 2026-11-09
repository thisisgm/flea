//! The rename replies: PaneWire.qml onRenamed, the refused write's reason, and the bulk card's paths, refusal and end.
use super::{leaf, parent, Shell};
use crate::{backend::Event as BackendEvent, window::navigate::Step};
use flea::ui_logic_status_feedback::errors;
use gpui::{Context, Window};

impl Shell {
    /// The backend's rename answer, its refusal or its end, before any other owner reads the line.
    pub(crate) fn rename_event(
        &mut self,
        side: usize,
        event: &BackendEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.opener_event(side, event, window, cx) {
            return true;
        }
        if let BackendEvent::Paths(paths) = event {
            if self.rename.paths == Some(side) {
                self.rename.paths = None;
                self.bulk_rename_open(side, paths.clone(), window, cx);
                return true;
            }
        }
        if let BackendEvent::Producer(line) | BackendEvent::Operations(line) = event {
            if self.bulk_rename_line(side, line, window, cx) {
                return true;
            }
        }
        if self.rename.sent[side] {
            match event {
                BackendEvent::Refused {
                    operation, message, ..
                } if operation == "bulkrename" => {
                    self.rename.sent[side] = false;
                    self.rename_say(errors::capitalised(message), true, cx);
                    return true;
                }
                BackendEvent::Drained | BackendEvent::Failed(_) => self.rename.sent[side] = false,
                _ => {}
            }
        }
        if self.rename.paths == Some(side) {
            match event {
                BackendEvent::Refused { operation, .. } if operation == "paths" => {
                    self.rename.paths = None
                }
                BackendEvent::Drained | BackendEvent::Failed(_) => self.rename.paths = None,
                _ => {}
            }
        }
        let Some(edit) = self.rename.edit.as_ref().filter(|edit| edit.side == side) else {
            return false;
        };
        let Some(destination) = edit.pending.clone() else {
            return false;
        };
        match event {
            BackendEvent::Producer(line) | BackendEvent::Operations(line) => {
                // Sample input: {"t":"renamed","ok":true,"path":"/files/draft.txt"}
                let Ok(doc) = flea::jsondoc::parse(line) else {
                    return false;
                };
                if doc.get("t").and_then(flea::jsondoc::Json::as_str) != Some("renamed") {
                    return false;
                }
                let path = doc.get("path").and_then(flea::jsondoc::Json::as_str);
                // SlowOp.js closesRename: only the reply naming this request's destination closes it.
                if path.is_some_and(|path| path != destination) {
                    return false;
                }
                self.rename_landed(side, Some(destination), window, cx);
                true
            }
            BackendEvent::Refused {
                operation,
                path,
                message,
                ..
            } => {
                let source = edit.source.clone();
                let ours = path.is_empty()
                    || *path == source
                    || *path == destination
                    || path.starts_with(&format!("{source}/"));
                if !ours
                    || !matches!(
                        operation.as_str(),
                        "rename" | "journal" | "rename-kept" | "rename-stranded"
                    )
                {
                    return false;
                }
                if operation == "journal" && *path == destination {
                    let said = self.locale.catalog.tr("jobs.rename.journal");
                    self.rename_say(said, true, cx);
                    self.rename_landed(side, Some(destination), window, cx);
                } else if operation != "rename" {
                    self.rename_say(errors::capitalised(message), true, cx);
                    self.rename_landed(side, None, window, cx);
                } else {
                    let reason = if errors::exists(message) {
                        self.locale
                            .catalog
                            .tr_args("jobs.rename.exists", &[leaf(&destination)])
                    } else if let Some(refusal) = errors::drive_refusal(message) {
                        // A drive's name refusal reads in the window's language, as the status line says it.
                        refusal.render(&self.locale.catalog)
                    } else {
                        errors::capitalised(message)
                    };
                    self.rename_refused(reason, cx);
                }
                true
            }
            BackendEvent::Drained | BackendEvent::Failed(_) => {
                let said = self.locale.catalog.tr("jobs.rename.stopped");
                self.rename_say(said, true, cx);
                self.rename.edit = None;
                false
            }
            _ => false,
        }
    }

    /// The write landed: the editor closes and the folder is read again with the new name under the cursor.
    fn rename_landed(
        &mut self,
        side: usize,
        destination: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(edit) = self.rename.edit.take() else {
            return;
        };
        if self.rename.neighbour.take().is_some() {
            if edit.menu != 0 {
                self.menu_send(
                    side,
                    format!(r#"{{"c":"menuaction","op":"close","id":{}}}"#, edit.menu),
                    cx,
                );
            }
            self.rename_neighbour_landed(side, &edit.source, destination.as_deref(), window, cx);
            return;
        }
        // Nav.js renameRefreshTarget: a cursor moved off the row meanwhile keeps its own row.
        let moved = self.panes[side]
            .as_ref()
            .is_none_or(|host| host.pane.cursor != edit.index);
        let folder = parent(&edit.source).trim_end_matches('/');
        let here = self.panes[side].as_ref().is_some_and(|host| {
            host.pane.path().trim_end_matches('/') == folder
                || (host.pane.held.listing == edit.listing
                    && host.pane.held.row(edit.index).is_some_and(|row| {
                        row.depth > 0 && super::join(host.pane.path(), &row.name) == edit.source
                    }))
        });
        self.rename_sync_gate(window, cx);
        if here {
            // A cursor a key moved off the renamed row keeps its own row through an anchored re-read (PaneWire.qml refreshRename).
            let step = match destination {
                _ if moved => Step::Reread,
                Some(select) => Step::Refresh(Some(select)),
                None => Step::Refresh(None),
            };
            if self.step(side, step, window, cx) == Ok(true) {
                if let Some(host) = self.panes[side].as_mut() {
                    host.nav.watch.clear();
                }
                self.arm_watch(side, window, cx);
            }
        }
        cx.notify();
    }

    /// A refused write keeps the draft open with the reason under it, or says it in the status with no editor.
    pub(super) fn rename_refused(&mut self, reason: String, cx: &mut Context<Self>) {
        let catalog = self.locale.catalog.clone();
        let Some(edit) = self.rename.edit.as_mut() else {
            return;
        };
        edit.pending = None;
        edit.error = if reason.is_empty() {
            catalog.tr("feedback.error.rename")
        } else {
            reason
        };
        // With no live editor the refusal goes to the status bar and the edit closes.
        if !edit.entry.editing {
            let text = edit.error.clone();
            self.rename.edit = None;
            self.rename_say(text, true, cx);
        }
        cx.notify();
    }
}
