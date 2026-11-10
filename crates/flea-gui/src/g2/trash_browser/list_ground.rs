//! Which ground a Trash row draws under its text, read from the paint state the pane last wrote.
use super::board::{HOVER, WASH};
use super::list::ListPaint;
use gpui::Hsla;

impl ListPaint {
    /// The marked cursor row's rung and accent edge, drawn only while the list has the keyboard.
    pub fn edge_at(&self, index: usize) -> bool {
        // TrashView.qml grounds marked rows only, so a cursor row v unmarked draws no rung (EY-388).
        index == self.cursor && self.edge && self.line(index).is_some_and(|line| line.marked)
    }

    /// The row's ground: the cursor rung while the list has the keyboard, the grey cursor while the keys are away, the marked plate, or the pointer wash.
    pub fn ground(&self, index: usize) -> Option<Hsla> {
        let marked = self.line(index).is_some_and(|line| line.marked);
        if self.edge_at(index) {
            Some(self.ink.accent.opacity(WASH))
        } else if marked && index == self.cursor && self.away {
            Some(self.ink.foreground.opacity(crate::list::GREY_CURSOR))
        } else if marked {
            Some(self.ink.mark_fill)
        } else if self.hover_wash && self.hover == Some(index) {
            Some(self.ink.foreground.opacity(HOVER))
        } else {
            None
        }
    }
}
