//! Keyboard geometry comes from the active painted Columns track.
use super::PaneHost;
use crate::columns::geometry::Slot;

impl PaneHost {
    pub(crate) fn columns_row_height(&self) -> f32 {
        if self.list.borrow().view.contents {
            crate::g2::sr1::contents::BLOCK_H
        } else {
            crate::pane::ROW_HEIGHT
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
