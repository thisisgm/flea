//! Rows without lstat facts use the existing menu snapshot before opening their editor.
use super::Shell;
use crate::backend::Event;
use flea::{
    json::escape,
    jsondoc::{self, Json},
    ui_logic_status_feedback::errors,
};
use gpui::{Context, Window};

pub(super) struct Opening {
    pub side: usize,
    listing: usize,
    index: usize,
    source: String,
    id: u64,
}

impl Shell {
    pub(super) fn rename_capture(
        &mut self,
        side: usize,
        source: String,
        cx: &mut Context<Self>,
    ) -> bool {
        self.rename_capture_cancel(cx);
        let Some(host) = self.panes[side].as_ref() else {
            return false;
        };
        self.menu.sequence += 1;
        let id = self.menu.sequence;
        let open = Opening {
            side,
            listing: host.pane.held.listing,
            index: host.pane.cursor,
            source,
            id,
        };
        let source = escape(&open.source);
        let request = format!(
            r#"{{"c":"menuaction","op":"snapshot","id":{id},"listing":{},"paths":["{source}"],"cursorPath":"{source}"}}"#,
            open.listing
        );
        self.rename.opening = Some(open);
        self.menu_send(side, request, cx);
        true
    }

    pub(super) fn rename_capture_cancel(&mut self, cx: &mut Context<Self>) {
        if let Some(open) = self.rename.opening.take() {
            self.menu_send(
                open.side,
                format!(r#"{{"c":"menuaction","op":"close","id":{}}}"#, open.id),
                cx,
            );
        }
    }

    pub(super) fn rename_capture_event(
        &mut self,
        side: usize,
        event: &Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(open) = self
            .rename
            .opening
            .as_ref()
            .filter(|open| open.side == side)
        else {
            return false;
        };
        if matches!(event, Event::Drained | Event::Failed(_)) {
            self.rename_capture_cancel(cx);
            self.rename_sync_gate(window, cx);
            return false;
        }
        let (Event::Producer(line) | Event::Operations(line)) = event else {
            return false;
        };
        // Sample input: {"t":"menuaction","id":7,"op":"snapshot","ok":false,"error":"Selected items changed; reopen the menu."}
        let Ok(doc) = jsondoc::parse(line) else {
            return false;
        };
        if doc.get("t").and_then(Json::as_str) != Some("menuaction")
            || doc.get("op").and_then(Json::as_str) != Some("snapshot")
            || flea::json::field_usize(line, "id").map(|id| id as u64) != Some(open.id)
        {
            return false;
        }
        let open = self.rename.opening.take().unwrap();
        let live =
            self.focus_side == side
                && self.rename.edit.is_none()
                && self.panes[side].as_ref().is_some_and(|host| {
                    host.pane.held.listing == open.listing
                        && host.pane.cursor == open.index
                        && host.pane.held.row(open.index).is_some_and(|row| {
                            super::join(host.pane.path(), &row.name) == open.source
                        })
                });
        if live && doc.get("ok").and_then(Json::as_bool) == Some(true) {
            self.rename_open(side, open.id, window, cx);
            if !self
                .rename
                .edit
                .as_ref()
                .is_some_and(|edit| edit.menu == open.id)
            {
                self.menu_send(
                    side,
                    format!(r#"{{"c":"menuaction","op":"close","id":{}}}"#, open.id),
                    cx,
                );
            }
        } else {
            self.menu_send(
                side,
                format!(r#"{{"c":"menuaction","op":"close","id":{}}}"#, open.id),
                cx,
            );
            if live {
                let message = doc.get("error").and_then(Json::as_str).unwrap_or("");
                let reason = super::replies::identity_refusal(&self.locale.catalog, message)
                    .unwrap_or_else(|| errors::capitalised(message));
                self.rename_say(reason, true, cx);
            }
        }
        self.rename_sync_gate(window, cx);
        cx.notify();
        true
    }
}
