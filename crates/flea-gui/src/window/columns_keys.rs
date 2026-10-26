//! Keyboard geometry comes from the active painted Columns track.
use super::PaneHost;
use crate::columns::geometry::Slot;

impl PaneHost {
    /// Keyboard moves measure the active track that was drawn, including a standing filter.
    pub(crate) fn columns_key_frame(&self) -> Option<crate::input::Frame> {
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
        Some(crate::input::Frame {
            bounds: drawn.bounds,
            row: crate::pane::ROW_HEIGHT,
            lane: 0.,
            content: total as f32 * crate::pane::ROW_HEIGHT,
            scroll: self.pane.scroll_y,
            listing: self.pane.held.listing,
            total,
            columns: 1,
            cell_width: drawn.bounds.size.width.into(),
        })
    }
}
