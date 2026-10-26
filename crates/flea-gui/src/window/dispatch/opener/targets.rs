//! Resolve a marked selection or an image gallery against the backend's own numbering.
use super::*;
use crate::{backend::Event, g2::mn1::join::Chosen};
use flea::{
    jsondoc::{self, Json},
    ui_logic_fb1::OpenRefusal,
    ui_logic_op3::viewerorder,
};

pub(super) enum Pending {
    Selection {
        id: u64,
        listing: usize,
        path: String,
        rows: Vec<usize>,
    },
    Gallery {
        ask: viewerorder::Ask,
        held: viewerorder::Held,
        first: String,
    },
}

impl Shell {
    pub(super) fn open_selection(
        &mut self,
        side: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(host) = self.panes[side].as_ref() else {
            return false;
        };
        if host.selected.len() < 2 {
            return false;
        }
        if host.pane.busy() || host.stopped {
            return true;
        }
        if self.opener[side].pending.is_some() || self.opener[side].guard.pending() {
            let text = busy_text(&self.locale.catalog);
            self.opener_say(side, text, false, cx);
            return true;
        }
        let mut rows = flea::ui_logic_sr2::keep(&host.selected, host.filter.shown.as_deref());
        rows.sort_unstable();
        rows.dedup();
        if rows.is_empty() {
            return true;
        }
        if let Some(refusal) = rows.iter().find_map(|&index| {
            host.pane.held.row(index).and_then(|row| {
                if row.dir {
                    Some(OpenRefusal::FolderWithFiles)
                } else if quick_look_kind(&row.icon, &row.name, false) == State::Archive
                    && !row.name.to_ascii_lowercase().ends_with(".appimage")
                {
                    Some(OpenRefusal::Archives)
                } else {
                    None
                }
            })
        }) {
            self.opener_refused(side, refusal, cx);
            return true;
        }
        let listing = host.pane.held.listing;
        let path = host.pane.path().to_owned();
        let cursor = host.pane.cursor;
        self.menu.sequence += 1;
        let id = self.menu.sequence;
        let indices = rows
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(",");
        self.opener[side].pending = Some(Pending::Selection {
            id,
            listing,
            path,
            rows,
        });
        self.opener_request(side, format!(r#"{{"c":"menuaction","op":"snapshot","id":{id},"listing":{listing},"rows":[{indices}],"cursor":{cursor}}}"#), cx);
        let _ = window;
        true
    }

    pub(super) fn open_menu_selection(
        &mut self,
        chosen: &Chosen,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let side = chosen.captured.side;
        let current = self.panes[side].as_ref().is_some_and(|host| {
            !host.pane.busy()
                && !host.stopped
                && host.pane.held.listing == chosen.captured.listing
                && host.pane.path() == chosen.captured.path
        });
        if current {
            self.open_paths(side, chosen.paths.clone(), true, window, cx);
        }
    }

    pub(super) fn open_gallery(
        &mut self,
        side: usize,
        first: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(host) = self.panes[side].as_ref() else {
            return false;
        };
        let Some(row) = host.pane.held.row(host.pane.cursor) else {
            return false;
        };
        if quick_look_kind(&row.icon, &row.name, false) != State::Image {
            return false;
        }
        if self.opener[side].pending.is_some()
            || self.opener[side].guard.pending()
            || host.nav.gate.paths_pending
            || host.nav.gate.clip_pending
            || self.rename.paths == Some(side)
            || self.carry_pending(side, window, cx)
        {
            let text = busy_text(&self.locale.catalog);
            self.opener_say(side, text, false, cx);
            return true;
        }
        let ask = viewerorder::ask(
            host.pane.cursor,
            host.pane.held.listing as u64,
            &host.selected,
            host.filter.shown.as_deref(),
        );
        let held = viewerorder::Held {
            listing: ask.listing,
            path: host.pane.path().to_owned(),
            in_flight: false,
        };
        let sent = host
            .backend
            .as_ref()
            .is_some_and(|backend| backend.line(ask.request()).is_ok());
        if sent {
            self.panes[side]
                .as_mut()
                .expect("the gallery pane")
                .nav
                .gate
                .paths_pending = true;
            self.opener[side].pending = Some(Pending::Gallery { ask, held, first });
        } else {
            self.opener_refused(side, OpenRefusal::NotLoaded, cx);
        }
        true
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
        }) = self.opener[side].pending.as_ref()
        else {
            return false;
        };
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
        if doc.get("ok").and_then(Json::as_bool) != Some(true) || !current {
            self.open_selection_end(side, id, cx);
            self.opener_refused(side, OpenRefusal::NotLoaded, cx);
        } else if op == "snapshot" {
            self.opener_request(
                side,
                format!(r#"{{"c":"menuaction","op":"validate","id":{id},"action":"open"}}"#),
                cx,
            );
        } else {
            let paths: Option<Vec<String>> =
                doc.get("paths").and_then(Json::as_array).and_then(|paths| {
                    paths
                        .iter()
                        .map(|path| path.as_str().map(str::to_owned))
                        .collect()
                });
            let count = rows.len();
            self.open_selection_end(side, id, cx);
            match paths.filter(|paths| paths.len() == count) {
                Some(paths) => self.open_paths(side, paths, true, window, cx),
                None => self.opener_refused(side, OpenRefusal::NotLoaded, cx),
            }
        }
        true
    }

    fn open_selection_end(&mut self, side: usize, id: u64, cx: &mut Context<Self>) {
        self.opener[side].pending = None;
        self.opener_request(
            side,
            format!(r#"{{"c":"menuaction","op":"close","id":{id}}}"#),
            cx,
        );
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
