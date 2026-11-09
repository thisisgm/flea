//! Grid rereads preserve the cursor tile's painted offset using the current tile layout.
use super::super::Shown;
use crate::{navigation::anchor::Viewport, window::pane::PaneHost};

impl PaneHost {
    pub(crate) fn grid_anchor_viewport(&self) -> Option<Viewport> {
        let grid = self.grid.as_ref().filter(|grid| grid.view.visible)?;
        let shown = self
            .filter
            .shown
            .as_deref()
            .map_or(Shown::All(self.pane.held.total), Shown::Only);
        let layout = grid.view.layout(shown);
        Some(Viewport {
            columns: layout.columns,
            row_height: layout.cell_height,
            origin: layout.row_top(0),
            scroll: grid.view.drawn_scroll(),
            shown: self.filter.shown.clone(),
        })
    }

    pub(crate) fn prepare_grid_anchor(&mut self) {
        self.nav.viewport = self.grid_anchor_viewport();
    }

    /// A settings relist (hidden files, sort, grouping) holds the Grid cursor tile's viewport row, as a reload does.
    pub(crate) fn reread_preferences(&mut self) -> bool {
        self.prepare_grid_anchor();
        self.nav.reread_preferences(&mut self.pane, &self.selected)
    }

    /// GridArea.qml placeCursor on a cleared filter: Contain from the painted filtered scroll while the strip still stands, then the reflow keeps it.
    pub(crate) fn grid_filter_cleared(&mut self, filtered: &[usize]) {
        let Some(grid) = self.grid.as_mut().filter(|grid| grid.view.visible) else {
            return;
        };
        grid.view.scroll = grid
            .view
            .layout(Shown::Only(filtered))
            .clamp(grid.view.drawn_scroll());
        grid.view.glide = None;
        let layout = grid.view.layout(Shown::All(self.pane.held.total));
        grid.view.reveal(&layout, self.pane.cursor);
    }

    pub(super) fn restore_grid_anchor(&mut self) {
        let Some((listing, cursor, offset)) = self.nav.grid_landing else {
            return;
        };
        if listing != self.pane.held.listing || cursor != self.pane.cursor {
            self.nav.grid_landing = None;
            return;
        }
        if self.pane.held.row(cursor).is_none() {
            return;
        }
        let Some(grid) = self.grid.as_mut() else {
            return;
        };
        let shown = self
            .filter
            .shown
            .as_deref()
            .map_or(Shown::All(self.pane.held.total), Shown::Only);
        let layout = grid.view.layout(shown);
        let view = shown.view_of(cursor).unwrap_or(0);
        grid.view.scroll = layout.clamp(layout.row_top(view / layout.columns) - offset);
        grid.view.glide = None;
        grid.view.landing_wait = false;
        grid.view.landing_held = None;
        grid.followed = Some((listing, cursor));
        self.nav.grid_landing = None;
    }
}
