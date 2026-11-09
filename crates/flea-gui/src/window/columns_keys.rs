//! Keyboard geometry comes from the active painted Columns track.
use super::PaneHost;
use crate::columns::geometry::Slot;

impl PaneHost {
    pub(crate) fn columns_row_height(&self) -> f32 {
        self.columns_rows().0
    }

    /// The active track's pitch and the band its row centres in: Row density's row, or a contents block over its result row.
    pub(crate) fn columns_rows(&self) -> (f32, f32) {
        let list = self.list.borrow();
        if list.view.contents {
            (
                crate::g2::file_search::contents::BLOCK_H,
                crate::g2::file_search::contents::ROW_H,
            )
        } else {
            (list.columns_row(), list.columns_row())
        }
    }

    /// Keyboard moves measure the active track that was drawn, including a standing filter.
    pub(crate) fn columns_key_frame(
        &self,
        grow: crate::list::pane::Grow,
    ) -> Option<crate::input::Frame> {
        let columns = self.columns.as_ref()?;
        let paint = columns.painters.get(Slot::Active)?.borrow();
        let drawn = paint.drawn.as_ref()?;
        if !std::rc::Rc::ptr_eq(&drawn.tracks, columns.area.tracks())
            || drawn.held.as_ref()?.listing != self.pane.held.listing
        {
            return None;
        }
        let total = drawn
            .shown
            .as_ref()
            .map_or(self.pane.held.total, |rows| rows.len());
        let row = drawn.row_height;
        Some(crate::input::Frame {
            bounds: drawn.bounds,
            row,
            lane: 0.,
            content: crate::columns::geometry::active_content(grow, total, row, &paint.typeface),
            scroll: self.pane.scroll_y,
            listing: self.pane.held.listing,
            total,
            columns: 1,
            cell_width: drawn.bounds.size.width.into(),
            grow,
            cell_origin: (0., 0.),
        })
    }
}
