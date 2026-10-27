//! Marked rows resolve through the backend's existing menu snapshot and validation, never through the held viewport.
use crate::{backend::Event, state::Shell};
use flea::json::{field_bool, field_str, field_str_array, field_usize, field_usize_array};
use gpui::{Context, Window};

// The core handoff's imageopen::MAX_IMAGES count bound also caps the snapshot request.
const MAX_MARKED: usize = 4096;
const DIRECTORY_MODE: usize = 0o040000;
const RESOLVE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

pub(super) struct Capture {
    id: u64,
    listing: usize,
    path: String,
    rows: Vec<usize>,
    validating: bool,
    snapshot: bool,
    preview: Option<(usize, String)>,
}

impl Shell {
    pub(super) fn open_selected(
        &mut self,
        side: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(host) = self.panes[side].as_ref() else {
            return false;
        };
        if host.selected.is_empty() || host.selected.as_slice() == [host.pane.cursor] {
            return false;
        }
        if host.stopped || host.pane.busy() {
            return true;
        }
        if self.opener[side].selection.is_some() || self.opener[side].guard.pending() {
            self.opener_say(
                side,
                flea::ui_logic_op3::opener::busy_text(&self.locale.catalog),
                false,
                cx,
            );
            return true;
        }
        if host.selected.len() > MAX_MARKED {
            self.opener_say(
                side,
                self.locale.catalog.tr("dialogs.file.open.failed"),
                true,
                cx,
            );
            return true;
        }
        self.menu.sequence += 1;
        let capture = Capture {
            id: self.menu.sequence,
            listing: host.pane.held.listing,
            path: host.path.clone(),
            rows: host.selected.clone(),
            validating: false,
            snapshot: true,
            preview: None,
        };
        let rows = capture
            .rows
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let line = format!(
            r#"{{"c":"menuaction","op":"snapshot","id":{},"listing":{},"rows":[{rows}]}}"#,
            capture.id, capture.listing
        );
        let id = capture.id;
        self.opener[side].selection = Some(capture);
        if !self.opener_selection_send(side, line) {
            self.finish_open_selection(side, Some("dialogs.file.open.failed"), cx);
            return true;
        }
        self.watch_open_selection(side, id, window, cx);
        true
    }

    fn watch_open_selection(
        &mut self,
        side: usize,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.opener[side].task = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(RESOLVE_TIMEOUT).await;
            let _ = this.update_in(cx, |shell, _, cx| {
                if shell.opener[side]
                    .selection
                    .as_ref()
                    .is_some_and(|capture| capture.id == id)
                {
                    shell.finish_open_selection(side, Some("feedback.refusal.not_loaded"), cx);
                }
            });
        }));
    }

    // Sample input: {"t":"menuaction","op":"snapshot","id":1,"ok":true,"kinds":[32768,32768]}
    pub(crate) fn opener_event(
        &mut self,
        side: usize,
        event: &Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(capture) = self.opener[side].selection.as_ref() else {
            return false;
        };
        if matches!(event, Event::Failed(_) | Event::Drained) {
            self.finish_open_selection(side, Some("dialogs.file.open.failed"), cx);
            return false;
        }
        let Event::Producer(line) = event else {
            return false;
        };
        if field_str(line, "t").as_deref() != Some("menuaction")
            || field_usize(line, "id").map(|n| n as u64) != Some(capture.id)
        {
            return false;
        }
        let current = self.panes[side].as_ref().is_some_and(|host| {
            !host.stopped
                && !host.pane.busy()
                && host.pane.held.listing == capture.listing
                && host.path == capture.path
                && host.selected == capture.rows
        });
        if !current {
            self.finish_open_selection(side, None, cx);
            return true;
        }
        if !field_bool(line, "ok") {
            self.finish_open_selection(side, Some("dialogs.file.open.failed"), cx);
            return true;
        }
        match field_str(line, "op").as_deref() {
            Some("snapshot") if !capture.validating => {
                let kinds = field_usize_array(line, "kinds");
                let key = if kinds.len() != capture.rows.len() {
                    Some("feedback.refusal.not_loaded")
                } else if kinds.len() > 1 && kinds.contains(&DIRECTORY_MODE) {
                    Some("feedback.refusal.folder_with_files")
                } else {
                    None
                };
                if key.is_some() {
                    self.finish_open_selection(side, key, cx);
                } else {
                    let id = capture.id;
                    self.opener[side].selection.as_mut().unwrap().validating = true;
                    if !self.opener_selection_send(
                        side,
                        format!(
                            r#"{{"c":"menuaction","op":"validate","id":{id},"action":"open"}}"#
                        ),
                    ) {
                        self.finish_open_selection(side, Some("dialogs.file.open.failed"), cx);
                    }
                }
            }
            Some("validate")
                if capture.validating && field_str(line, "action").as_deref() == Some("open") =>
            {
                let paths = field_str_array(line, "paths");
                let complete = paths.len() == capture.rows.len()
                    && paths.iter().all(|path| path.starts_with('/'));
                if complete && paths.len() == 1 && super::archive_path(&paths[0]) {
                    let index = capture.rows[0];
                    self.opener[side].selection.as_mut().unwrap().preview =
                        Some((index, paths[0].clone()));
                    self.reveal_open_archive(side, index, window, cx);
                    return true;
                }
                self.finish_open_selection(
                    side,
                    if complete {
                        None
                    } else {
                        Some("feedback.refusal.not_loaded")
                    },
                    cx,
                );
                if complete {
                    self.open_files(side, paths, window, cx);
                }
            }
            _ => {}
        }
        true
    }

    pub(super) fn open_archive_path(
        &mut self,
        side: usize,
        index: usize,
        path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.opener[side].selection.is_some() || self.opener[side].guard.pending() {
            self.opener_say(
                side,
                flea::ui_logic_op3::opener::busy_text(&self.locale.catalog),
                false,
                cx,
            );
            return;
        }
        let Some(host) = self.panes[side].as_ref() else {
            return;
        };
        self.menu.sequence += 1;
        let id = self.menu.sequence;
        self.opener[side].selection = Some(Capture {
            id,
            listing: host.pane.held.listing,
            path: host.path.clone(),
            rows: host.selected.clone(),
            validating: true,
            snapshot: false,
            preview: Some((index, path)),
        });
        self.watch_open_selection(side, id, window, cx);
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
        let Some(capture) = self.opener[side].selection.as_ref() else {
            return;
        };
        let Some((index, path)) = capture.preview.as_ref() else {
            return;
        };
        // The bare preview below acts on the focused pane's cursor, so a wait on an unfocused pane ends quietly.
        if side != self.focus_side {
            self.finish_open_selection(side, None, cx);
            return;
        }
        let Some(host) = self.panes[side].as_mut() else {
            return;
        };
        if host.stopped
            || host.pane.busy()
            || host.pane.held.listing != capture.listing
            || host.path != capture.path
            || host.selected != capture.rows
            || host.pane.cursor != *index
        {
            self.finish_open_selection(side, None, cx);
            return;
        }
        if *index >= host.pane.held.total {
            self.finish_open_selection(side, Some("feedback.refusal.not_loaded"), cx);
            return;
        }
        if let Some(row) = host.pane.held.row(*index) {
            if row.dir || crate::navigation::history::join(&host.path, &row.name) != *path {
                self.finish_open_selection(side, Some("feedback.refusal.not_loaded"), cx);
                return;
            }
        } else {
            if let (Some((start, count)), Some(backend)) =
                (host.pane.wanted_window(), host.backend.as_ref())
            {
                if backend.window(start, count).is_err() {
                    self.finish_open_selection(side, Some("feedback.refusal.not_loaded"), cx);
                }
            }
            return;
        }
        self.finish_open_selection(side, None, cx);
        self.dispatch(
            super::Order::Action(crate::window::input::ActionEvent::bare(
                "preview",
                flea::keys::Context::Listing,
            )),
            window,
            cx,
        );
    }

    fn opener_selection_send(&self, side: usize, line: String) -> bool {
        self.panes[side]
            .as_ref()
            .and_then(|host| host.backend.as_ref())
            .is_some_and(|backend| backend.line(line).is_ok())
    }

    #[cfg(test)]
    pub(crate) fn open_selection_parked(&self, side: usize) -> bool {
        self.opener[side].selection.is_some()
    }

    fn finish_open_selection(&mut self, side: usize, key: Option<&str>, cx: &mut Context<Self>) {
        if let Some(capture) = self.opener[side]
            .selection
            .take()
            .filter(|capture| capture.snapshot)
        {
            self.opener_selection_send(
                side,
                format!(r#"{{"c":"menuaction","op":"close","id":{}}}"#, capture.id),
            );
        }
        self.opener[side].task = None;
        if let Some(key) = key {
            self.opener_say(
                side,
                self.locale.catalog.tr(key),
                key == "dialogs.file.open.failed",
                cx,
            );
        }
    }
}
