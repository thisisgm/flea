//! Own deletes keep the first deleted row's slot and select its replacement after the reread.
use crate::{navigation::anchor::Anchor, state::Shell};
use gpui::{Context, Window};

impl Shell {
    pub(super) fn deletion_capture(&self, side: usize, rows: &[usize]) -> Option<Anchor> {
        let host = self.panes.get(side)?.as_ref()?;
        let first = *rows.iter().min()?;
        let mut anchor = Anchor::capture(&host.pane, &[], true, false);
        anchor.index = first;
        anchor.offset = first as f32 * host.pane.row_height() - host.pane.scroll_y;
        anchor.grid_offset = host.grid_anchor_viewport().map(|view| view.offset(first));
        anchor.name = host
            .pane
            .held
            .row(first)
            .map_or_else(String::new, |row| row.name.clone());
        Some(anchor)
    }

    pub(super) fn deletion_landed(
        &mut self,
        side: usize,
        anchor: Option<Anchor>,
        landed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.panes.get_mut(side).and_then(Option::as_mut) else {
            return;
        };
        if anchor.as_ref().is_some_and(|anchor| {
            anchor.path != host.pane.path() || anchor.path != host.pane.place()
        }) {
            return;
        }
        host.selected.clear();
        let Some(anchor) = anchor.filter(|anchor| {
            anchor.path == host.pane.path()
                && anchor.path == host.pane.place()
                && !host.pane.state.search
        }) else {
            return;
        };
        let anchor = if landed {
            anchor
        } else {
            let mut anchor = Anchor::capture(&host.pane, &[], true, false);
            anchor.grid_offset = host
                .grid_anchor_viewport()
                .map(|view| view.offset(host.pane.cursor));
            anchor
        };
        host.nav.deletion_anchor = Some(anchor);
        if host.pane.busy() {
            let path = host.pane.path().to_owned();
            host.nav.changed(&host.pane, &path, host.pane.now());
        } else {
            host.nav.watch.clear();
            host.nav.reread(&mut host.pane, &[], false, true);
        }
        self.run_navigation(side, window, cx);
        self.arm_watch(side, window, cx);
        cx.notify();
    }
}
