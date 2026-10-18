use super::{ListState, RowCache};

impl ListState {
    pub fn invalidate_text(&mut self) {
        self.rows = RowCache::default();
        self.advance = None;
        self.visible.clear();
    }
}
