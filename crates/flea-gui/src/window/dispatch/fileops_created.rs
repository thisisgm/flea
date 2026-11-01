//! A created neighbour item may take focus only while its request's pane state remains current.
use crate::{g2::mn1::join::identity, state::Shell, window::navigate::Step};
use gpui::{Context, Window};
use std::collections::VecDeque;

const MAX_PENDING_CREATIONS: usize = 64;

struct Origin {
    request: u64,
    kind: &'static str,
    parent: String,
    identity: String,
    background: bool,
}

#[derive(Default)]
pub(super) struct Targets([VecDeque<Origin>; 2]);

impl Targets {
    pub(super) fn clear(&mut self, side: usize) {
        self.0[side].clear();
    }

    pub(super) fn failed(&mut self, side: usize, kind: &str, request: u64) {
        self.0[side].retain(|origin| origin.kind != kind || origin.request != request);
    }
}

impl Shell {
    // Sample inputs: {"c":"duplicate","path":"/a/f"}, {"c":"mkdir","path":"/b"}.
    pub(super) fn fileops_creation_request(
        &mut self,
        side: usize,
        line: String,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let command = flea::json::field_str(&line, "c").unwrap_or_default();
        let Some(path) = flea::json::field_str(&line, "path") else {
            return Some(line);
        };
        let (kind, parent) = match command.as_str() {
            "duplicate" => ("duplicated", crate::navigation::history::parent_of(&path)),
            "mkdir" => ("made", path.as_str()),
            _ => return Some(line),
        };
        let prefix = line.trim_end().strip_suffix('}')?;
        let request = self.fileops_creation_requested(side, kind, parent, cx)?;
        // Preserve all independent fields, including an optional localized mkdir defaultBase.
        Some(format!("{prefix},\"request\":{request}}}"))
    }

    pub(crate) fn fileops_creation_requested(
        &mut self,
        side: usize,
        kind: &'static str,
        parent: &str,
        cx: &mut Context<Self>,
    ) -> Option<u64> {
        let host = self.panes[side].as_ref()?;
        self.menu.sequence += 1;
        let request = self.menu.sequence;
        let background = kind != "duplicated";
        let origin = Origin {
            request,
            kind,
            parent: parent.to_owned(),
            identity: identity(host, side, background),
            background,
        };
        let pending = &mut self.operations.fileops.creations.0[side];
        // New File has one modal owner; a canceled card cannot leave an unbounded receipt queue.
        if kind == "newFile" {
            pending.retain(|origin| origin.kind != kind);
        }
        if pending.len() >= MAX_PENDING_CREATIONS {
            let text = self.locale.catalog.tr("pane.busy");
            self.fileops_say(side, &text, false, cx);
            return None;
        }
        pending.push_back(origin);
        Some(request)
    }

    pub(super) fn fileops_created_target(
        &mut self,
        side: usize,
        kind: &str,
        request: Option<u64>,
        path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let parent = crate::navigation::history::parent_of(&path).to_owned();
        let pending = &mut self.operations.fileops.creations.0[side];
        let origin = pending
            .iter()
            .position(|origin| {
                origin.kind == kind
                    && request.map_or(origin.parent == parent, |id| origin.request == id)
            })
            .and_then(|at| pending.remove(at));
        let Some(host) = self.panes[side].as_ref() else {
            return false;
        };
        if (request.is_some() && origin.is_none())
            || origin
                .as_ref()
                .is_some_and(|origin| origin.parent != parent)
            || origin
                .as_ref()
                .is_some_and(|origin| origin.identity != identity(host, side, origin.background))
            || (host.path != parent && origin.is_none())
        {
            return false;
        }
        if host.path == parent {
            let arrived = (kind == "made")
                .then(|| self.operations.fileops.arrival[side].replace(path.clone()));
            let landed = self.step(side, Step::Refresh(Some(path)), window, cx) == Ok(true);
            if !landed {
                if let Some(previous) = arrived {
                    self.operations.fileops.arrival[side] = previous;
                }
            }
            return landed;
        }
        let arrived =
            (kind == "made").then(|| self.operations.fileops.arrival[side].replace(path.clone()));
        let previous = self.panes[side]
            .as_mut()
            .and_then(|host| host.nav.pending_select.replace(path));
        match self.step(side, Step::OpenPlace(parent), window, cx) {
            Ok(true) => true,
            _ => {
                if let Some(host) = self.panes[side].as_mut() {
                    host.nav.pending_select = previous;
                }
                if let Some(previous) = arrived {
                    self.operations.fileops.arrival[side] = previous;
                }
                false
            }
        }
    }
}
