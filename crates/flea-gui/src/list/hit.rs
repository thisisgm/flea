//! Tap.onName: a press hits the drawn glyphs of a row's name, never the empty rest of the name slot.
use super::ListState;
use gpui::{px, Pixels};

impl ListState {
    /// Whether x lands on the name the last frame drew for this listing row.
    pub fn on_name(&self, index: usize, x: Pixels) -> bool {
        let Some(shown) = self.visible.iter().find(|shown| shown.index == index) else {
            return false;
        };
        if shown.state.renaming {
            return false;
        }
        let Some(width) = self.drawn_name_width(index) else {
            return false;
        };
        let left = self.drawn.bounds.origin.x
            + px(self.drawn.layout.name.x
                + self
                    .drawn_rows
                    .row(index)
                    .map_or(0., |row| super::outline::indent(row, &self.drawn.metrics)));
        x >= left && x < left + px(width)
    }
}
