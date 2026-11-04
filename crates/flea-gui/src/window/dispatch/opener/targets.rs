//! Resolve a marked selection or an image gallery against the backend's own numbering.
use super::*;
use crate::{backend::Event, g2::mn1::join::Chosen};
use flea::{
    jsondoc::{self, Json},
    ui_logic_fb1::OpenRefusal,
    ui_logic_op3::viewerorder,
};

// The core handoff's imageopen::MAX_IMAGES count bound also caps the snapshot request.
const MAX_MARKED: usize = 4096;
const FILE_TYPE: usize = 0o170000;
const DIRECTORY_MODE: usize = 0o040000;
const RESOLVE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

pub(super) enum Pending {
    Selection {
        id: u64,
        listing: usize,
        path: String,
        rows: Vec<usize>,
        // None awaits the snapshot; Some retains its first row kind until validation.
        kind: Option<usize>,
    },
    // One validated archive waits for its own row, since Quick Look reads the cursor row; close names a snapshot this lane still holds.
    Archive {
        id: u64,
        listing: usize,
        path: String,
        rows: Vec<usize>,
        index: usize,
        target: String,
        close: bool,
    },
    Gallery {
        ask: viewerorder::Ask,
        held: viewerorder::Held,
        first: String,
    },
}

impl Shell {
    fn open_archive_path(
        &mut self,
        side: usize,
        index: usize,
        target: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.opener[side].pending.is_some() || self.opener[side].guard.pending() {
            let text = busy_text(&self.locale.catalog);
            self.opener_say(side, text, false, cx);
            return;
        }
        let Some(host) = self.panes[side].as_ref() else {
            return;
        };
        self.menu.sequence += 1;
        let id = self.menu.sequence;
        self.opener[side].pending = Some(Pending::Archive {
            id,
            listing: host.pane.held.listing,
            path: host.pane.path().to_owned(),
            rows: host.selected.clone(),
            index,
            target,
            close: false,
        });
        self.opener_watch(side, id, window, cx);
        self.reveal_open_archive(side, index, window, cx);
    }

    fn reveal_open_archive(
        &mut self,
        side: usize,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(host) = self.panes[side].as_mut() {
            host.pane.cursor = index;
            host.pane.reveal_cursor();
        }
        self.opener_archive_ready(side, window, cx);
        cx.notify();
    }

    // Quick Look reads the cursor row, so an offscreen validated archive waits for the existing bounded row request.
    pub(crate) fn opener_archive_ready(
        &mut self,
        side: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(Pending::Archive {
            id,
            listing,
            path,
            rows,
            index,
            target,
            ..
        }) = self.opener[side].pending.as_ref()
        else {
            return;
        };
        let (id, index) = (*id, *index);
        // The bare preview below acts on the focused pane's cursor, so a wait on an unfocused pane ends quietly.
        if side != self.focus_side {
            self.open_selection_end(side, id, cx);
            return;
        }
        let Some(host) = self.panes[side].as_mut() else {
            return;
        };
        if host.stopped
            || host.pane.busy()
            || host.pane.held.listing != *listing
            || host.pane.path() != path
            || host.selected != *rows
            || host.pane.cursor != index
        {
            self.open_selection_end(side, id, cx);
            return;
        }
        if index >= host.pane.held.total {
            self.open_selection_end(side, id, cx);
            self.opener_refused(side, OpenRefusal::NotLoaded, cx);
            return;
        }
        let Some(row) = host.pane.held.row(index) else {
            let wanted = host.pane.wanted_window();
            if let (Some((start, count)), Some(backend)) = (wanted, host.backend.as_ref()) {
                if backend.window(start, count).is_err() {
                    self.open_selection_end(side, id, cx);
                    self.opener_refused(side, OpenRefusal::NotLoaded, cx);
                }
            }
            return;
        };
        if row.dir || join(host.pane.path(), &row.name) != *target {
            self.open_selection_end(side, id, cx);
            self.opener_refused(side, OpenRefusal::NotLoaded, cx);
            return;
        }
        self.open_selection_end(side, id, cx);
        self.dispatch(
            Order::Action(ActionEvent::bare("preview", Keys::Listing)),
            window,
            cx,
        );
    }

    #[cfg(test)]
    pub(crate) fn open_selection_parked(&self, side: usize) -> bool {
        self.opener[side].pending.is_some()
    }

    pub(crate) fn opener_event(
        &mut self,
        side: usize,
        event: &Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.opener[side].pending.is_none() {
            return false;
        }
        if matches!(event, Event::Failed(_) | Event::Drained) {
            self.opener_release(side);
            self.opener[side].wait = None;
            return false;
        }
        match event {
            Event::Paths(paths)
                if matches!(self.opener[side].pending, Some(Pending::Gallery { .. })) =>
            {
                let Some(Pending::Gallery { ask, held, first }) = self.opener_release(side) else {
                    return false;
                };
                let Some(host) = self.panes[side].as_ref() else {
                    return true;
                };
                let now = viewerorder::Held {
                    listing: host.pane.held.listing as u64,
                    path: host.pane.path().to_owned(),
                    in_flight: host.pane.busy() || host.stopped,
                };
                match viewerorder::deliver(&ask, &held, &now, paths.clone()) {
                    viewerorder::Delivered::Open { paths, .. } => {
                        self.open_paths(side, paths, false, window, cx)
                    }
                    viewerorder::Delivered::Fallback => self.open_file(side, first, window, cx),
                    viewerorder::Delivered::Nothing => {}
                }
                true
            }
            Event::Refused { operation, .. }
                if matches!(operation.as_str(), "paths" | "stale" | "menuaction") =>
            {
                if operation != "paths" {
                    if let Some(Pending::Selection { id, .. }) = self.opener[side].pending.as_ref()
                    {
                        let id = *id;
                        self.open_selection_end(side, id, cx);
                        self.opener_refused(side, OpenRefusal::NotLoaded, cx);
                        return true;
                    }
                }
                if matches!(self.opener[side].pending, Some(Pending::Gallery { .. })) {
                    self.opener_release(side);
                    self.opener_refused(side, OpenRefusal::NotLoaded, cx);
                    return true;
                }
                false
            }
            Event::Producer(line) | Event::Operations(line) => {
                self.open_selection_reply(side, line, window, cx)
            }
            _ => false,
        }
    }

    fn opener_release(&mut self, side: usize) -> Option<Pending> {
        let pending = self.opener[side].pending.take();
        if matches!(pending, Some(Pending::Gallery { .. })) {
            if let Some(host) = self.panes[side].as_mut() {
                host.nav.gate.paths_pending = false;
            }
        }
        pending
    }

    fn open_selection_reply(
        &mut self,
        side: usize,
        line: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(Pending::Selection {
            id,
            listing,
            path,
            rows,
            kind,
        }) = self.opener[side].pending.as_ref()
        else {
            return false;
        };
        let validating = kind.is_some();
        let directory = kind.is_some_and(|kind| kind & FILE_TYPE == DIRECTORY_MODE);
        let Ok(doc) = jsondoc::parse(line) else {
            return false;
        };
        if doc.get("t").and_then(Json::as_str) != Some("menuaction")
            || flea::json::field_usize(line, "id").map(|id| id as u64) != Some(*id)
        {
            return false;
        }
        let id = *id;
        let op = doc.get("op").and_then(Json::as_str).unwrap_or("");
        if !matches!(op, "snapshot" | "validate") {
            return false;
        }
        let current = self.panes[side].as_ref().is_some_and(|host| {
            !host.pane.busy()
                && !host.stopped
                && host.pane.held.listing == *listing
                && host.pane.path() == path
                && {
                    let mut now =
                        flea::ui_logic_sr2::keep(&host.selected, host.filter.shown.as_deref());
                    now.sort_unstable();
                    now.dedup();
                    now == *rows
                }
        });
        // A reply out of turn (a second snapshot, a validate before one) is the producer's echo and changes nothing.
        if (op == "snapshot") == validating
            || (op == "validate" && doc.get("action").and_then(Json::as_str) != Some("open"))
        {
            return true;
        }
        if doc.get("ok").and_then(Json::as_bool) != Some(true) || !current {
            self.open_selection_end(side, id, cx);
            self.opener_refused(side, OpenRefusal::NotLoaded, cx);
        } else if op == "snapshot" {
            // Sample input: {"t":"menuaction","op":"snapshot","id":1,"ok":true,"kinds":[33188,16877]}
            let kinds = flea::json::field_usize_array(line, "kinds");
            let refusal = if kinds.len() != rows.len() {
                Some(OpenRefusal::NotLoaded)
            } else if kinds.len() > 1 && kinds.iter().any(|kind| kind & FILE_TYPE == DIRECTORY_MODE)
            {
                Some(OpenRefusal::FolderWithFiles)
            } else {
                None
            };
            if let Some(refusal) = refusal {
                self.open_selection_end(side, id, cx);
                self.opener_refused(side, refusal, cx);
            } else {
                if let Some(Pending::Selection { kind, .. }) = self.opener[side].pending.as_mut() {
                    *kind = kinds.first().copied();
                }
                self.opener_request(
                    side,
                    format!(r#"{{"c":"menuaction","op":"validate","id":{id},"action":"open"}}"#),
                    cx,
                );
            }
        } else {
            let paths: Option<Vec<String>> =
                doc.get("paths").and_then(Json::as_array).and_then(|paths| {
                    paths
                        .iter()
                        .map(|path| path.as_str().map(str::to_owned))
                        .collect()
                });
            let count = rows.len();
            let first = rows[0];
            let paths = paths.filter(|paths| {
                paths.len() == count && paths.iter().all(|path| path.starts_with('/'))
            });
            match paths {
                // One marked archive stays inside Flea: Quick Look opens on its own row once that row is loaded.
                Some(paths) if count == 1 && !directory && archive_path(&paths[0]) => {
                    let (listing, path, rows) = (*listing, path.clone(), rows.clone());
                    self.opener[side].pending = Some(Pending::Archive {
                        id,
                        listing,
                        path,
                        rows: self.panes[side]
                            .as_ref()
                            .map(|host| host.selected.clone())
                            .unwrap_or(rows),
                        index: first,
                        target: paths[0].clone(),
                        close: true,
                    });
                    self.reveal_open_archive(side, first, window, cx);
                }
                // One marked file opens the way Enter opens it, its cards included.
                Some(paths) => {
                    self.open_selection_end(side, id, cx);
                    self.open_paths(side, paths.clone(), count > 1, window, cx);
                }
                None => {
                    self.open_selection_end(side, id, cx);
                    self.opener_refused(side, OpenRefusal::NotLoaded, cx);
                }
            }
        }
        true
    }

    fn open_selection_end(&mut self, side: usize, id: u64, cx: &mut Context<Self>) {
        let pending = self.opener[side].pending.take();
        self.opener[side].wait = None;
        // A menu's archive wait holds no snapshot, and a stopped backend's failed close is not an error.
        if !matches!(pending, Some(Pending::Archive { close: false, .. })) {
            if let Some(backend) = self.panes[side]
                .as_ref()
                .and_then(|host| host.backend.as_ref())
            {
                let _ =
                    backend.operation(format!(r#"{{"c":"menuaction","op":"close","id":{id}}}"#));
            }
        }
        cx.notify();
    }

    fn opener_request(&mut self, side: usize, line: String, cx: &mut Context<Self>) {
        let result = self.panes[side]
            .as_ref()
            .and_then(|host| host.backend.as_ref())
            .ok_or(crate::backend::Failure {
                key: "window.backend_stopped",
                detail: String::new(),
            })
            .and_then(|backend| backend.operation(line));
        if let Err(error) = result {
            self.opener_release(side);
            if let Some(host) = self.panes[side].as_mut() {
                host.record_error(error, &self.locale);
            }
            cx.notify();
        }
    }

    fn opener_refused(&mut self, side: usize, refusal: OpenRefusal, cx: &mut Context<Self>) {
        let sentence = self.locale.catalog.tr(refusal.key());
        self.opener_say(side, sentence, false, cx);
    }
}

#[path = "targets_open.rs"]
mod open;
