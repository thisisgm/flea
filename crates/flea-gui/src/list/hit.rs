//! Tap.onName: a press hits the drawn glyphs of a row's name, never the empty rest of the name slot.
use super::ListState;
use gpui::{px, Pixels};

impl ListState {
    /// Whether x lands on the name the last frame drew for this listing row.
    pub fn on_name(&self, index: usize, x: Pixels) -> bool {
        let Some(prepared) = self
            .visible
            .iter()
            .find(|shown| shown.index == index && !shown.state.renaming)
            .and_then(|shown| shown.prepared.as_ref())
        else {
            return false;
        };
        let name = self.drawn.layout.name;
        let left = self.drawn.bounds.origin.x + px(name.x);
        let width = prepared.name.line().width.min(px(name.width));
        x >= left && x < left + width
    }
}
