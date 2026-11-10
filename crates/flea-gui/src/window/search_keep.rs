//! A same-query replacement resolves the source listing's full file identities in the backend.
use crate::{backend::Failure, navigation::anchor::Anchor, state::Shell};
use flea::{json, protocol::Reply};
use gpui::{Context, Window};

pub(crate) struct Kept {
    generation: usize,
    listing: usize,
    anchor: Anchor,
    lone: bool,
}

impl super::super::pane::PaneHost {
    pub(crate) fn search_reply_dropped(
        &mut self,
        generation: usize,
        reply: &Reply,
        locale: &crate::window::locale::Snapshot,
    ) -> bool {
        if self.pane.swap.session.drops(generation) {
            return true;
        }
        if let Err(error) = self.keep_search(generation, reply) {
            self.record_error(error, locale);
        }
        false
    }

    pub(crate) fn keep_search(&mut self, generation: usize, reply: &Reply) -> Result<(), Failure> {
        let Reply::Listed { path, .. } = reply else {
            return Ok(());
        };
        if self.search.kept.is_some()
            || !self.pane.state.search
            || self.path != *path
            || self.search.rows_generation != generation
            || self.stopped
        {
            return Ok(());
        }
        let Some(backend) = &self.backend else {
            return Ok(());
        };
        let mut anchor = Anchor::capture(&self.pane, &[], false, false);
        anchor.grid_offset = self
            .grid_anchor_viewport()
            .map(|view| view.offset(self.pane.cursor));
        let rows = self
            .selected
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(",");
        backend.line(format!(
            r#"{{"c":"searchremap","gen":{generation},"from":{},"rows":[{rows}],"cursor":{}}}"#,
            self.pane.held.listing, self.pane.cursor
        ))?;
        self.search.kept = Some(Kept {
            generation,
            listing: self.pane.held.listing,
            anchor,
            lone: self.input.selection.is_lone(),
        });
        Ok(())
    }
}

impl Shell {
    pub(crate) fn search_reread(
        &mut self,
        side: usize,
        step: &crate::window::navigate::Step,
    ) -> Option<Result<bool, crate::navigation::Refusal>> {
        use crate::window::navigate::Step;
        if !matches!(step, Step::Reread | Step::Refresh(_)) {
            return None;
        }
        let host = self.panes[side]
            .as_mut()
            .filter(|host| host.search.owner.active())?;
        if let (Some(backend), Some(request)) = (&host.backend, host.pane.reread_held()) {
            if let Err(error) = backend.window(request.start, request.count) {
                host.pane.requests.cancel();
                host.record_error(error, &self.locale);
            }
        }
        Some(Ok(true))
    }

    // {"t":"searchremap","ok":true,"gen":7,"from":12,"listing":15,"rows":[8],"cursor":8}
    pub(crate) fn search_remapped(
        &mut self,
        side: usize,
        line: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.panes[side].as_mut() else {
            return;
        };
        let Some(kept) = host.search.kept.as_ref() else {
            return;
        };
        if json::field_usize(line, "gen") != Some(kept.generation)
            || json::field_usize(line, "from") != Some(kept.listing)
        {
            return;
        }
        // The backend answers the one request in flight once, so even an unappliable answer ends the wait.
        let mut kept = host.search.kept.take().unwrap();
        if !json::field_bool(line, "ok")
            || !host.pane.state.search
            || json::field_usize(line, "listing") != Some(host.pane.held.listing)
            || host.pane.swap.session.drops(kept.generation)
            || host.stopped
        {
            return;
        }
        let total = host.pane.held.total;
        host.selected = json::field_usize_array(line, "rows")
            .into_iter()
            .filter(|&at| at < total)
            .collect();
        host.selected.sort_unstable();
        host.selected.dedup();
        let cursor = json::field_usize(line, "cursor").unwrap_or(kept.anchor.index);
        kept.anchor.land(&mut host.pane, cursor);
        host.nav.grid_landing = kept
            .anchor
            .grid_offset
            .map(|offset| (host.pane.held.listing, host.pane.cursor, offset));
        if kept.lone && host.selected == [host.pane.cursor] {
            host.input
                .selection
                .only(&mut host.selected, host.pane.cursor);
        } else {
            host.input.selection.changed(&host.selected);
        }
        self.search_rows_landed(side, window, cx);
        cx.notify();
    }
}
