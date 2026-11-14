//! List folders share the pane's numbering, anchor restoration and ordinary backend list path.
use crate::{state::Shell, window::startup::View};
use gpui::{Context, Pixels, Window};

pub(crate) enum Input {
    /// Every marked folder, or the cursor's folder when no marked row is one.
    Key,
    /// A click on this row's mark: that folder alone, and the cursor stays where it was.
    Mark(usize),
}

/// An outline line waits for the re-list of the folder it was made for, and only that one.
pub(crate) struct Request {
    pub(super) folder: String,
    pub(super) line: String,
    pub(super) mode: bool,
    /// An open of a row, which the setting can still refuse while the request waits for its listing.
    pub(super) open: bool,
}

impl Shell {
    pub(crate) fn outline_action(
        &mut self,
        side: usize,
        open: bool,
        input: Input,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.view != View::List
            || self.input_blocked(cx)
            || (open && crate::list::outline::off(&self.launch.settings.data))
        {
            return;
        }
        let Some(host) = self.panes[side].as_mut() else {
            return;
        };
        if host.outline_request.is_some()
            || host.pane.busy()
            || host.recent.on
            || host.pane.state.search
            || host.filter.owner.up()
        {
            return;
        }
        let mut gate = host.nav.gate;
        let held = &host.pane.held;
        // A marked row outside the held window goes to the backend unchecked, which falls back to the anchor if none is a folder.
        let folder = |index: &usize| {
            held.row(*index)
                .is_none_or(|row| row.dir && row.link.is_none())
        };
        let changes = |index: &usize| held.row(*index).is_none_or(|row| row.open != open);
        let mut rows: Vec<usize> = match input {
            Input::Mark(row) => {
                gate.pointer_held = false;
                vec![row]
            }
            Input::Key => host.selected.clone(),
        };
        rows.retain(folder);
        if matches!(input, Input::Key) && rows.is_empty() {
            rows = vec![host.pane.cursor];
            rows.retain(folder);
        }
        rows.sort_unstable();
        rows.dedup();
        if gate.owned() || !rows.iter().any(changes) {
            return;
        }
        let anchor = host.pane.cursor;
        let list = rows
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(",");
        host.outline_used = true;
        host.outline_lone = (host.input.selection.is_lone() && host.selected.len() == 1)
            .then_some(host.pane.held.listing);
        host.outline_request = Some(Request {
            folder: host.pane.path().to_owned(),
            line: format!(
                "{{\"c\":\"outline\",\"rows\":[{list}],\"open\":{open},\"anchor\":{anchor},\"listing\":{},\"first\":{}}}",
                host.pane.held.listing,
                host.pane.first_count()
            ),
            mode: false,
            open,
        });
        if !host
            .nav
            .reread(&mut host.pane, &host.selected, false, false)
        {
            host.outline_request = None;
            host.outline_lone = None;
            return;
        }
        self.run_navigation(side, window, cx);
        cx.notify();
    }

    pub(crate) fn outline_hit(&self, side: usize, row: usize, x: Pixels) -> bool {
        if self.view != View::List {
            return false;
        }
        let Some(host) = self.panes[side].as_ref() else {
            return false;
        };
        if host.pane.busy() || host.recent.on || host.pane.state.search {
            return false;
        }
        let list = host.list.borrow();
        let metrics = list.drawn.metrics;
        let Some(right) = list
            .drawn_rows
            .row(row)
            .and_then(|facts| crate::list::outline::mark_target(&list.view, facts, &metrics))
        else {
            return false;
        };
        let left = list.drawn.bounds.origin.x;
        x >= left && x < left + gpui::px(right)
    }

    pub(crate) fn outline_mode(&mut self, view: View, window: &mut Window, cx: &mut Context<Self>) {
        for side in 0..2 {
            let Some(host) = self.panes[side].as_mut() else {
                continue;
            };
            let enabled = view == View::List;
            if !host.outline_used {
                continue;
            }
            host.outline_request = Some(Request {
                folder: host.pane.path().to_owned(),
                line: format!(
                    "{{\"c\":\"outline\",\"enabled\":{enabled},\"first\":{}}}",
                    host.pane.first_count()
                ),
                mode: true,
                open: false,
            });
            if host.pane.busy() || host.search.owner.active() || host.pane.state.search {
                let sent = match (&host.backend, host.outline_request.take()) {
                    (Some(backend), Some(request)) => backend.line(request.line),
                    _ => Ok(()),
                };
                if let Err(error) = sent {
                    host.stopped = true;
                    host.record_error(error, &self.locale);
                }
            } else if host
                .nav
                .reread(&mut host.pane, &host.selected, false, false)
            {
                self.run_navigation(side, window, cx);
            } else {
                host.outline_request = None;
            }
        }
    }
}

// An in-place open or close re-lists by identity; a lone mark stays the one a plain move carries.
pub(super) fn keep_lone(host: &mut super::pane::PaneHost) {
    if host
        .outline_lone
        .is_some_and(|left| left != host.pane.held.listing)
    {
        host.outline_lone = None;
        if let [row] = host.selected[..] {
            host.input.selection.only(&mut host.selected, row);
        }
    }
}

pub(super) fn send_navigation(
    host: &mut super::pane::PaneHost,
    command: &crate::navigation::Command,
) -> Option<Result<(), crate::backend::Failure>> {
    let request = match command {
        crate::navigation::Command::List(listing) => host
            .outline_request
            .take()
            .map(|request| (request.folder == listing.path, request)),
        _ => None,
    };
    let backend = host.backend.as_ref()?;
    // Settings > View "Open folders in place" switched off while an open waited: the plain list goes instead.
    let refused = request
        .as_ref()
        .is_some_and(|(_, request)| request.open && host.list.borrow().view.outline_off);
    match request {
        Some((true, request)) if !refused => return Some(backend.line(request.line)),
        // A list of another folder drops a row toggle; a view switch still reaches the backend before it.
        Some((false, request)) if request.mode => {
            if let Err(error) = backend.line(request.line) {
                return Some(Err(error));
            }
        }
        _ => {}
    }
    Some(backend.command(command))
}
