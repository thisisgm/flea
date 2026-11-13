use super::{ListState, RowCache};

impl ListState {
    pub(crate) fn retain_folder_sizes(&mut self, sizes: &crate::navigation::sort::DirSizes) {
        let before = self.view.walked.len();
        self.view
            .walked
            .retain(|row, _| sizes.size_for(*row).is_some());
        if before != self.view.walked.len() {
            self.view.walked_revision += 1;
        }
    }

    /// State surfaces still occupy a measured body, but no row from the previous frame is drawn.
    pub fn hide_rows(&mut self, bounds: gpui::Bounds<gpui::Pixels>, scroll_y: f32, listing: usize) {
        self.visible.clear();
        self.drawn.bounds = bounds;
        self.drawn.mask = bounds;
        self.drawn.metrics = super::Metrics::at(self.typeface.body);
        self.drawn.scroll_y = scroll_y;
        self.drawn.listing = listing;
    }

    pub fn invalidate_text(&mut self) {
        self.rows = RowCache::default();
        self.advance = None;
        self.visible.clear();
    }
}
