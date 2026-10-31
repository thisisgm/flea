//! List folders share the pane's numbering, anchor restoration and ordinary backend list path.
use crate::{state::Shell, window::startup::View};
use gpui::{Context, Pixels, Window};

pub(crate) enum Input {
    Key,
    Chevron,
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
        if self.view != View::List || self.input_blocked(cx) {
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
        let Some(row) = host.pane.held.row(host.pane.cursor) else {
            return;
        };
        let mut gate = host.nav.gate;
        if matches!(input, Input::Chevron) {
            gate.pointer_held = false;
        }
        if gate.owned() || !row.dir || row.link.is_some() || row.open == open {
            return;
        }
        host.outline_used = true;
        host.outline_lone = (host.input.selection.is_lone() && host.selected.len() == 1)
            .then_some(host.pane.held.listing);
        host.outline_request = Some(format!(
            "{{\"c\":\"outline\",\"row\":{},\"open\":{open},\"listing\":{},\"first\":{}}}",
            host.pane.cursor,
            host.pane.held.listing,
            host.pane.first_count()
        ));
        if !host
            .nav
            .reread(&mut host.pane, &host.selected, false, false)
        {
            host.outline_request = None;
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
        let Some(facts) = list
            .drawn_rows
            .row(row)
            .filter(|facts| facts.dir && (facts.open || facts.link.is_none()))
        else {
            return false;
        };
        let metrics = list.drawn.metrics;
        let left = list.drawn.bounds.origin.x
            + gpui::px(metrics.pad + crate::list::outline::indent(facts, &metrics));
        x >= left && x < left + gpui::px(metrics.slot)
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
            host.outline_request = Some(format!(
                "{{\"c\":\"outline\",\"enabled\":{enabled},\"first\":{}}}",
                host.pane.first_count()
            ));
            if host.pane.busy() || host.search.owner.active() || host.pane.state.search {
                if let (Some(backend), Some(line)) = (&host.backend, host.outline_request.take()) {
                    let _ = backend.line(line);
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
    if host.outline_lone.is_some_and(|left| left != host.pane.held.listing) {
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
    let backend = host.backend.as_ref()?;
    if matches!(command, crate::navigation::Command::List(_)) {
        if let Some(line) = host.outline_request.take() {
            return Some(backend.line(line));
        }
    }
    Some(backend.command(command))
}
